//! Hash updates for node-based sure file.

// Clippy bug.
#![allow(clippy::if_same_then_else)]

use crate::{
    hashes::{hash_file, noatime_open, Estimate},
    node::{into_tracker, NodeWriter, SureNode},
    progress::Progress,
    store::{Store, TempCleaner},
    Error, Result,
};
use crossbeam::channel::{bounded, Sender};
use data_encoding::HEXLOWER;
use log::{debug, error};
use rusqlite::{types::ToSql, Connection, NO_PARAMS};
use std::{
    io::Write,
    mem,
    path::PathBuf,
    sync::{mpsc::sync_channel, Arc, Mutex},
    thread,
};

/// A Source is something that can repeatedly give us an iterator over
/// nodes.
pub trait Source {
    fn iter(&self) -> Result<Box<dyn Iterator<Item = Result<SureNode>> + Send>>;
}

/// The HashUpdater is able to update hashes.  This is the first pass.
pub struct HashUpdater<'n, S> {
    source: S,
    store: &'n dyn Store,
}

pub struct HashMerger<S> {
    source: S,
    conn: Connection,
    // Own the temp, so it won't be deleted until the connection is also
    // closed.
    _temp: Box<dyn TempCleaner>,
}

impl<'a, S: Source> HashUpdater<'a, S> {
    pub fn new(source: S, store: &dyn Store) -> HashUpdater<S> {
        HashUpdater {
            source,
            store,
        }
    }

    /// First pass.  Go through the source nodes, and for any that need a
    /// hash, compute the hash, and collect the results into a temporary
    /// file.  Consumes the updater, returning the HashMerger which is used
    /// to merge the hash results into a datastream.
    pub fn compute(mut self, base: &str, estimate: &Estimate) -> Result<HashMerger<S>> {
        let meter = Arc::new(Mutex::new(Progress::new(estimate.files, estimate.bytes)));
        let (mut conn, temp) = self.setup_db()?;

        let (tx, rx) = sync_channel(num_cpus::get());

        let iter = into_tracker(self.source.iter()?, base);
        let mut count = 0;
        let meter2 = meter.clone();
        thread::spawn(move || {
            for entry in iter {
                let entry = entry.unwrap();
                if entry.node.needs_hash() {
                    let path = entry.path.unwrap();
                    match noatime_open(&path) {
                        Ok(mut fd) => match hash_file(&mut fd) {
                            Ok(ref h) => {
                                tx.send(Some(HashInfo {
                                    id: count,
                                    hash: h.as_ref().to_owned(),
                                }))
                                .unwrap();
                            }
                            Err(e) => {
                                error!("Unable to hash file: '{:?}' ({})", path, e);
                            }
                        },
                        Err(e) => {
                            error!("Unable to open '{:?}' for hashing ({})", path, e);
                        }
                    }
                    // println!("{} {:?}", count, entry.path);
                    count += 1;

                    meter2.lock().unwrap().update(1, entry.node.size());
                }
            }
            tx.send(None).unwrap();
        });

        // The above will send Option<HashInfo> over the tx/rx channel.
        // Capture these and add them all to the database.
        let trans = conn.transaction()?;
        while let Some(info) = rx.recv()? {
            trans.execute(
                "INSERT INTO hashes (id, hash) VALUES (?1, ?2)",
                &[&info.id as &dyn ToSql, &info.hash as &dyn ToSql],
            )?;
        }
        trans.commit()?;

        meter.lock().unwrap().flush();
        Ok(HashMerger {
            source: self.source,
            conn,
            _temp: temp,
        })
    }

    /// First pass, multi-threaded version.  Go through the source nodes,
    /// and for any that need a hash, compute the hash, and collect the
    /// result into a temporary file.  Consumes the updater, returning the
    /// HashMerger which is used to merge the hash results into a
    /// datastream.
    pub fn compute_parallel(mut self, base: &str, estimate: &Estimate) -> Result<HashMerger<S>> {
        let meter = Arc::new(Mutex::new(Progress::new(estimate.files, estimate.bytes)));
        let iter = into_tracker(self.source.iter()?, base);
        let (mut conn, temp) = self.setup_db()?;
        let trans = conn.transaction()?;

        let meter2 = meter.clone();
        crossbeam::scope(move |s| {
            let ncpu = num_cpus::get();

            // The work channel.  Single sender, multiple receivers (one
            // for each CPU).
            let (work_send, work_recv) = bounded(ncpu);

            // The result channel.  Multiple senders, single receiver.
            let (result_send, result_recv) = bounded(ncpu);

            // This thread reads the nodes, and submits work requests for
            // them.  This will close the channel when it finishes, as the
            // work_send is moved in.
            s.spawn(move |_| {
                let mut count = 0;
                for entry in iter {
                    let entry = entry.unwrap(); // TODO: Handle error.
                    if entry.node.needs_hash() {
                        let path = entry.path.unwrap();
                        work_send
                            .send(HashWork {
                                id: count,
                                path,
                                size: entry.node.size(),
                            })
                            .unwrap();
                        count += 1;
                    }
                }
            });

            // Fire off a thread for each worker.
            for _ in 0..ncpu {
                let work_recv = work_recv.clone();
                let result_send = result_send.clone();
                let meter2 = meter2.clone();
                s.spawn(move |_| {
                    for work in work_recv {
                        hash_one_file(&work, &result_send, &meter2);
                    }
                });
            }
            drop(result_send);

            // And, in the main thread, take all of the results, and add
            // them to the sql database.
            for info in result_recv {
                trans
                    .execute(
                        "INSERT INTO hashes (id, hash) VALUES (?1, ?2)",
                        &[&info.id as &dyn ToSql, &info.hash as &dyn ToSql],
                    )
                    .unwrap();
            }
            trans.commit()?;
            ok_result()
        })
        .map_err(|e| Error::Hash(format!("{:?}", e)))??;

        meter.lock().unwrap().flush();
        Ok(HashMerger {
            source: self.source,
            conn,
            _temp: temp,
        })
    }

    /// Set up the sqlite database to hold the hash updates.
    fn setup_db(&mut self) -> Result<(Connection, Box<dyn TempCleaner>)> {
        // Create the temp file.  Discard the file so that it will be
        // closed.
        let tmp = self.store.make_temp()?.into_loader()?;
        let conn = Connection::open(tmp.path_ref())?;
        conn.execute(
            "CREATE TABLE hashes (
                id INTEGER PRIMARY KEY,
                hash BLOB)",
            NO_PARAMS,
        )?;

        Ok((conn, tmp.into_cleaner()?))
    }
}

fn hash_one_file(work: &HashWork, sender: &Sender<HashInfo>, meter: &Arc<Mutex<Progress>>) {
    match noatime_open(&work.path) {
        Ok(mut fd) => match hash_file(&mut fd) {
            Ok(ref h) => {
                sender
                    .send(HashInfo {
                        id: work.id,
                        hash: h.as_ref().to_owned(),
                    })
                    .unwrap();
            }
            Err(e) => {
                error!("Unable to hash file: '{:?}' ({})", work.path, e);
            }
        },
        Err(e) => {
            error!("Unable to open '{:?}' for hashing ({})", work.path, e);
        }
    }
    meter.lock().unwrap().update(1, work.size);
}

// To make it easier to return a typed result.
fn ok_result() -> Result<()> {
    Ok(())
}

impl<S: Source> HashMerger<S> {
    /// Second pass.  Merge the updated hashes back into the data.  Note
    /// that this is 'push' based instead of 'pull' because there is a
    /// chain of lifetime dependencies from Connection->Statement->Rows and
    /// if we tried to return something holding the Rows iterator, the user
    /// would have to manage these lifetimes.
    pub fn merge<W: Write>(self, writer: &mut NodeWriter<W>) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, hash FROM hashes ORDER BY id")?;
        let mut hash_iter = stmt
            .query_map(NO_PARAMS, |row| {
                Ok(HashInfo {
                    id: row.get(0)?,
                    hash: row.get(1)?,
                })
            })?
            .peekable();

        let mut count = 0;
        for entry in self.source.iter()? {
            let mut entry = entry?;
            if entry.needs_hash() {
                let hnode = loop {
                    match hash_iter.peek() {
                        Some(Ok(hnode)) => {
                            if count == hnode.id {
                                let node = hash_iter.next().unwrap()?;
                                break Some(node);
                            } else if count < hnode.id {
                                // Node not present in hash, means we
                                // weren't able to compute a hash of the
                                // file.
                                break None;
                            } else {
                                panic!("Out of sequence hash");
                            }
                        }
                        Some(Err(e)) => {
                            return Err(Error::WrappedSql(format!("{:?}", e)));
                        }
                        None => break None,
                    }
                };

                if let Some(HashInfo { hash, .. }) = &hnode {
                    let hex = HEXLOWER.encode(hash);
                    entry.atts_mut().unwrap().insert("sha1".to_string(), hex);
                }

                count += 1;
            }
            writer.write_node(&entry)?;
            // println!("{:?}", entry);
        }

        Ok(())
    }
}

#[derive(Debug)]
struct HashInfo {
    id: i64,
    hash: Vec<u8>,
}

#[derive(Debug)]
struct HashWork {
    id: i64,
    size: u64,
    path: PathBuf,
}

/// An iterator that pulls hash from old nodes if the file is unchanged.
pub struct HashCombiner<Iold: Iterator, Inew: Iterator> {
    // This works like Peekable, but we keep the head in this structure and
    // swap it out to advance.  Because the nodes are a strict tree
    // traversal, we always have a node to view, which makes this simpler
    // to use than Peekable, where every call can return a node or a
    // failure.
    /// The current head of the left tree.
    left: SureNode,
    /// The current head of the right tree.
    right: SureNode,

    /// The iterator for the left node.
    left_iter: Iold,
    /// The iterator for the right node.
    right_iter: Inew,

    state: Vec<CombineState>,
    seen_root: bool,
}

#[derive(Debug)]
enum CombineState {
    // Discard one tree level on the left side, we are viewing the dir
    // nodes.
    LeftDirs,

    // We are passing through the tree on the right.  Visiting the dir
    // nodes.
    RightDirs,

    // We are in a common directory, visiting the dir nodes.
    SameDirs,

    // We are in a common directory, visiting the file nodes.
    SameFiles,
}

impl<Iold, Inew> HashCombiner<Iold, Inew>
where
    Iold: Iterator<Item = Result<SureNode>>,
    Inew: Iterator<Item = Result<SureNode>>,
{
    pub fn new(mut left_iter: Iold, mut right_iter: Inew) -> Result<HashCombiner<Iold, Inew>> {
        let left = match left_iter.next() {
            None => return Err(Error::EmptyLeftIterator),
            Some(Err(e)) => return Err(e),
            Some(Ok(node)) => node,
        };
        let right = match right_iter.next() {
            None => return Err(Error::EmptyRightIterator),
            Some(Err(e)) => return Err(e),
            Some(Ok(node)) => node,
        };

        Ok(HashCombiner {
            left,
            right,
            left_iter,
            right_iter,
            state: vec![],
            seen_root: false,
        })
    }

    /// Advance the left iterator, replacing 'left' with the new value, and
    /// returning that old value.  Returns the error from the iterator if
    /// that happened.  If we see the end of the iterator, places 'Leave'
    /// in the node, which should be the same as what was there.
    fn next_left(&mut self) -> Result<SureNode> {
        let next = match self.left_iter.next() {
            None => SureNode::Leave,
            Some(Ok(node)) => node,
            Some(Err(e)) => return Err(e),
        };

        Ok(mem::replace(&mut self.left, next))
    }

    /// Advance the right iterator, replacing 'right' with the new value, and
    /// returning that old value.  Returns the error from the iterator if
    /// that happened.  If we see the end of the iterator, places 'Leave'
    /// in the node, which should be the same as what was there.
    fn next_right(&mut self) -> Result<SureNode> {
        let next = match self.right_iter.next() {
            None => SureNode::Leave,
            Some(Ok(node)) => node,
            Some(Err(e)) => return Err(e),
        };

        Ok(mem::replace(&mut self.right, next))
    }
}

/// The result of one of the visitors.  Continue means to go ahead and
/// process the next nodes.  Return means that this result should be
/// returned.  Note that we handle the EoF case specially, so this is not
/// an option.
enum VisitResult {
    Continue,
    Node(SureNode),
}

macro_rules! vre {
    ($err:expr) => {
        Err($err)
    };
}

macro_rules! vro {
    ($result:expr) => {
        Ok(VisitResult::Node($result))
    };
}

// The iterator for the hash combiner.  This iterator lazily traverses two
// iterators that are assumed to be and old and new traversal of the same
// filesystem.  The output will be the same nodes as the new, but possibly
// with 'sha1' values carried over from the old tree when there is a
// sufficient match.
impl<Iold, Inew> Iterator for HashCombiner<Iold, Inew>
where
    Iold: Iterator<Item = Result<SureNode>>,
    Inew: Iterator<Item = Result<SureNode>>,
{
    type Item = Result<SureNode>;

    fn next(&mut self) -> Option<Result<SureNode>> {
        loop {
            // Handle the completion state separately, so we don't have as
            // many to deal with below.
            if self.seen_root && self.state.is_empty() {
                return None;
            }

            let vr = match self.state.pop() {
                None => self.visit_root(),
                Some(CombineState::SameDirs) => self.visit_samedir(),
                Some(CombineState::SameFiles) => self.visit_samefiles(),
                Some(CombineState::RightDirs) => self.visit_rightdirs(),
                Some(CombineState::LeftDirs) => self.visit_leftdirs(),
            };

            match vr {
                Ok(VisitResult::Continue) => (),
                Ok(VisitResult::Node(node)) => return Some(Ok(node)),
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

// The body, a method for each state.
impl<Iold, Inew> HashCombiner<Iold, Inew>
where
    Iold: Iterator<Item = Result<SureNode>>,
    Inew: Iterator<Item = Result<SureNode>>,
{
    fn visit_root(&mut self) -> Result<VisitResult> {
        if !self.left.is_enter() {
            vre!(Error::UnexpectedLeftNode)
        } else if !self.right.is_enter() {
            vre!(Error::UnexpectedRightNode)
        } else if self.left.name() != "__root__" {
            vre!(Error::IncorrectName)
        } else if self.right.name() != "__root__" {
            vre!(Error::IncorrectName)
        } else {
            let _ = self.next_left()?;
            let rnode = self.next_right()?;
            self.state.push(CombineState::SameDirs);
            self.seen_root = true;
            vro!(rnode)
        }
    }

    // Both trees are in the same directory, and we are looking at
    // directory nodes.
    fn visit_samedir(&mut self) -> Result<VisitResult> {
        // Handle the cases where they aren't finished together.
        debug!("visit samedir: {:?}, {:?}", self.left, self.right);
        match (self.left.is_sep(), self.right.is_sep()) {
            (true, true) => {
                // Both have finished with child directories.
                let _ = self.next_left()?;
                let rnode = self.next_right()?;
                // Push the new state.
                self.state.push(CombineState::SameFiles);
                vro!(rnode)
            }
            (false, false) => {
                // We are still visiting directories.  Assume it is well
                // formed, and we are only going to see Enter nodes.
                if self.left.name() == self.right.name() {
                    // This is the same directory, descend it.
                    self.state.push(CombineState::SameDirs);
                    self.state.push(CombineState::SameDirs);
                    let _ = self.next_left()?;
                    vro!(self.next_right()?)
                } else if self.left.name() < self.right.name() {
                    // A directory in the old tree we no longer have.
                    let _ = self.next_left()?;
                    self.state.push(CombineState::SameDirs);
                    self.state.push(CombineState::LeftDirs);
                    Ok(VisitResult::Continue)
                } else {
                    // A new directory entirely.
                    self.state.push(CombineState::SameDirs);
                    self.state.push(CombineState::RightDirs);
                    vro!(self.next_right()?)
                }
            }
            (false, true) => {
                // Old has an old directory no longer present.
                let _ = self.next_left()?;
                self.state.push(CombineState::SameDirs);
                self.state.push(CombineState::LeftDirs);
                Ok(VisitResult::Continue)
            }
            (true, false) => {
                // Directories present in new, not in old.
                self.state.push(CombineState::SameDirs);
                self.state.push(CombineState::RightDirs);
                vro!(self.next_right()?)
            }
        }
    }

    // Both trees are in the same directory, and we are looking at file
    // nodes.
    fn visit_samefiles(&mut self) -> Result<VisitResult> {
        debug!("visit samefiles: {:?}, {:?}", self.left, self.right);
        match (self.left.is_leave(), self.right.is_leave()) {
            (true, true) => {
                // Both are leaving at the same time, nothing to push onto
                // state.  Consume the nodes, and return the leave.
                let _ = self.next_left()?;
                vro!(self.next_right()?)
            }
            (true, false) => {
                self.state.push(CombineState::SameFiles);
                // New file added in new, not present in old.
                vro!(self.next_right()?)
            }
            (false, true) => {
                // File removed.
                self.state.push(CombineState::SameFiles);
                let _ = self.next_left()?;
                Ok(VisitResult::Continue)
            }
            (false, false) => {
                self.state.push(CombineState::SameFiles);

                // Two names within a directory.
                if self.left.name() == self.right.name() {
                    let left = self.next_left()?;
                    let mut right = self.next_right()?;
                    maybe_copy_sha(&left, &mut right);
                    vro!(right)
                } else if self.left.name() < self.right.name() {
                    // An old name no longer present.
                    let _ = self.next_left()?;
                    Ok(VisitResult::Continue)
                } else {
                    // A new name with no corresponding old name.
                    vro!(self.next_right()?)
                }
            }
        }
    }

    fn visit_rightdirs(&mut self) -> Result<VisitResult> {
        debug!("visit rightdirs: {:?}, {:?}", self.left, self.right);
        if self.right.is_sep() {
            // Since we don't care about files, or matching, no need for
            // self.state.push(CombineState::RightFiles)
            // the RightFiles state, just stay.
            self.state.push(CombineState::RightDirs);
        } else if self.right.is_enter() {
            self.state.push(CombineState::RightDirs);
            self.state.push(CombineState::RightDirs);
        } else if self.right.is_leave() {
            // No state change.
        } else {
            // Otherwise, stays the same.
            self.state.push(CombineState::RightDirs);
        }
        vro!(self.next_right()?)
    }

    fn visit_leftdirs(&mut self) -> Result<VisitResult> {
        debug!("visit rightdirs: {:?}, {:?}", self.left, self.right);
        if self.left.is_sep() {
            // Since we don't care about files, or matching, no need for
            // self.state.push(CombineState::RightFiles)
            // the RightFiles state, just stay.
            self.state.push(CombineState::LeftDirs);
        } else if self.left.is_enter() {
            self.state.push(CombineState::LeftDirs);
            self.state.push(CombineState::LeftDirs);
        } else if self.left.is_leave() {
            // No state change.
        } else {
            // Otherwise, stays the same.
            self.state.push(CombineState::LeftDirs);
        }
        let _ = self.next_left()?;
        Ok(VisitResult::Continue)
    }
}

fn maybe_copy_sha(left: &SureNode, right: &mut SureNode) {
    let latts = left.atts().unwrap();
    let ratts = right.atts_mut().unwrap();

    // If we already have a sha1, don't do anything.
    if ratts.contains_key("sha1") {
        return;
    }

    // Only compare regular files.
    if latts["kind"] != "file" || ratts["kind"] != "file" {
        return;
    }

    // Make sure inode and ctime are identical.
    if latts.get("ino") != ratts.get("ino") || latts.get("ctime") != ratts.get("ctime") {
        return;
    }

    // And only update if there is a sha1 to get.
    match latts.get("sha1") {
        None => (),
        Some(v) => {
            ratts.insert("sha1".to_string(), v.to_string());
        }
    }
}
