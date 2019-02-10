//! Hash updates for node-based sure file.

use crate::{
    progress::Progress,
    node::{
        SureNode,
        NodeWriter,
        into_tracker,
    },
    store::{Store, TempCleaner},
    Result,

    hashes::{Estimate, hash_file, noatime_open},
};
use crossbeam::{
    channel::{bounded, Sender},
};
use data_encoding::HEXLOWER;
use failure::format_err;
use log::{debug, error};
use rusqlite::{
    types::ToSql,
    Connection,
    NO_PARAMS,
};
use std::{
    io::Write,
    iter::Peekable,
    path::PathBuf,
    sync::{
        Arc,
        Mutex,
        mpsc::sync_channel,
    },
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

impl <'a, S: Source> HashUpdater<'a, S> {
    pub fn new(source: S, store: &dyn Store) -> HashUpdater<S> {
        HashUpdater {
            source: source,
            store: store,
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
                                })).unwrap();
                            }
                            Err(e) => {
                                error!("Unable to hash file: '{:?}' ({})", path, e);
                            }
                        }
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
                &[&info.id as &dyn ToSql,
                &info.hash as &dyn ToSql])?;
        }
        trans.commit()?;

        meter.lock().unwrap().flush();
        Ok(HashMerger {
            source: self.source,
            conn: conn,
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
                        work_send.send(HashWork {
                            id: count,
                            path: path,
                            size: entry.node.size(),
                        }).unwrap();
                        count += 1;
                    }
                }
            });

            // Fire off a thread for each worker.
            for _ in 0 .. ncpu {
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
                trans.execute(
                    "INSERT INTO hashes (id, hash) VALUES (?1, ?2)",
                    &[&info.id as &dyn ToSql,
                    &info.hash as &dyn ToSql]).unwrap();
            }
            trans.commit()?;
            ok_result()
        }).map_err(|e| format_err!("Hash error: {:?}", e))??;

        meter.lock().unwrap().flush();
        Ok(HashMerger {
            source: self.source,
            conn: conn,
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
            NO_PARAMS)?;

        Ok((conn, tmp.into_cleaner()?))
    }
}

fn hash_one_file(work: &HashWork, sender: &Sender<HashInfo>, meter: &Arc<Mutex<Progress>>) {
    match noatime_open(&work.path) {
        Ok(mut fd) => match hash_file(&mut fd) {
            Ok(ref h) => {
                sender.send(HashInfo {
                    id: work.id,
                    hash: h.as_ref().to_owned(),
                }).unwrap();
            }
            Err(e) => {
                error!("Unable to hash file: '{:?}' ({})", work.path, e);
            }
        }
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

impl <S: Source> HashMerger<S> {
    /// Second pass.  Merge the updated hashes back into the data.  Note
    /// that this is 'push' based instead of 'pull' because there is a
    /// chain of lifetime dependencies from Connection->Statement->Rows and
    /// if we tried to return something holding the Rows iterator, the user
    /// would have to manage these lifetimes.
    pub fn merge<W: Write>(self, writer: &mut NodeWriter<W>) -> Result<()> {
        let mut stmt = self.conn.prepare("SELECT id, hash FROM hashes ORDER BY id")?;
        let mut hash_iter = stmt
            .query_map(NO_PARAMS, |row| HashInfo {
                id: row.get(0),
                hash: row.get(1),
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
                            // TODO: Can we convert this error, rather than
                            // just printing it?
                            return Err(format_err!("sql error: {:?}", e));
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
    left: Peekable<Iold>,
    right: Peekable<Inew>,
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
    pub fn new(
        left: Iold,
        right: Inew,
    ) -> Result<HashCombiner<Iold, Inew>> {
        Ok(HashCombiner {
            left: left.peekable(),
            right: right.peekable(),
            state: vec![],
            seen_root: false,
        })
    }
}

/// The result of one of the visitors.  Continue means to go ahead and
/// process the next nodes.  Return means that this result should be
/// returned.  Note that we handle the EoF case specially, so this is not
/// an option.
enum VisitResult {
    Continue,
    Return(Result<SureNode>),
}

macro_rules! vre {
    ($err:expr) => (VisitResult::Return(Err($err)))
}

macro_rules! vro {
    ($result:expr) => (VisitResult::Return(Ok($result)))
}

// The iterator for the hash combiner.  This iterator lazily traverses two
// iterators that are assumed to be and old and new traversal of the same
// filesystem.  The output will be the same nodes as the new, but possibly
// with 'sha1' values carried over from the old tree when there is a
// sufficient match.
impl<Iold, Inew> Iterator for HashCombiner<Iold, Inew>
    where Iold: Iterator<Item = Result<SureNode>>,
          Inew: Iterator<Item = Result<SureNode>>
{
    type Item = Result<SureNode>;

    fn next(&mut self) -> Option<Result<SureNode>> {
        loop {
            // Handle the completion state separately, so we don't have as
            // many to deal with below.
            if self.seen_root && self.state.is_empty() {
                return None;
            }

            // Peek each of left and right.  We handle the end state
            // specially above, so these should never be past the end.
            // TODO: Can we do this without cloning?  The problem is that
            // peek() borrows a mutable reference, and there isn't any way
            // to tell Rust that the resulting reference no longer needs
            // the mutable borrow.  For now, we just clone the nodes, which
            // is less efficient.
            let left = match self.left.peek() {
                None => return Some(Err(format_err!("Unexpected early end on older tree"))),
                Some(Err(e)) => return Some(Err(format_err!("Error reading older stream: {:?}", e))),
                Some(Ok(node)) => node.clone(),
            };
            let right = match self.right.peek() {
                None => return Some(Err(format_err!("Unexpected early end on newer tree"))),
                Some(Err(e)) => return Some(Err(format_err!("Error reading newer stream: {:?}", e))),
                Some(Ok(node)) => node.clone(),
            };

            let vr = match self.state.pop() {
                None => self.visit_root(&left, &right),
                Some(CombineState::SameDirs) => self.visit_samedir(&left, &right),
                Some(CombineState::SameFiles) => self.visit_samefiles(&left, &right),
                Some(CombineState::RightDirs) => self.visit_rightdirs(&left, &right),
                Some(CombineState::LeftDirs) => self.visit_leftdirs(&left, &right),
            };

            match vr {
                VisitResult::Continue => (),
                VisitResult::Return(item) => return Some(item),
            }
        }
        // TODO: Implement combine algorithm.
        // self.newer.next()
    }
}

// The body, a method for each state.
impl<Iold, Inew> HashCombiner<Iold, Inew>
    where Iold: Iterator<Item = Result<SureNode>>,
          Inew: Iterator<Item = Result<SureNode>>
{
    fn visit_root(&mut self, left: &SureNode, right: &SureNode) -> VisitResult {
        if !left.is_enter() {
            vre!(format_err!("Unexpected node in old tree"))
        } else if !right.is_enter() {
            vre!(format_err!("Unexpected node in new tree"))
        } else if left.name() != "__root__" {
            vre!(format_err!("Old tree root is incorrect name"))
        } else if right.name() != "__root__" {
            vre!(format_err!("New tree root is incorrect name"))
        } else {
            self.left.next().unwrap().unwrap();
            let rnode = self.right.next().unwrap().unwrap();
            self.state.push(CombineState::SameDirs);
            self.seen_root = true;
            vro!(rnode)
        }
    }

    // Both trees are in the same directory, and we are looking at
    // directory nodes.
    fn visit_samedir(&mut self, left: &SureNode, right: &SureNode) -> VisitResult {
        // Handle the cases where they aren't finished together.
        debug!("visit samedir: {:?}, {:?}", left, right);
        match (left.is_sep(), right.is_sep()) {
            (true, true) => {
                // Both have finished with child directories.
                let _ = self.left.next();
                let rnode = self.right.next().unwrap().unwrap();
                // Push the new state.
                self.state.push(CombineState::SameFiles);
                vro!(rnode)
            }
            (false, false) => {
                // We are still visiting directories.  Assume it is well
                // formed, and we are only going to see Enter nodes.
                if left.name() == right.name() {
                    // This is the same directory, descend it.
                    self.state.push(CombineState::SameDirs);
                    self.state.push(CombineState::SameDirs);
                    let _ = self.left.next();
                    vro!(self.right.next().unwrap().unwrap())
                } else if left.name() < right.name() {
                    // A directory in the old tree we no longer have.
                    let _ = self.left.next();
                    self.state.push(CombineState::SameDirs);
                    self.state.push(CombineState::LeftDirs);
                    VisitResult::Continue
                } else {
                    // A new directory entirely.
                    self.state.push(CombineState::SameDirs);
                    self.state.push(CombineState::RightDirs);
                    vro!(self.right.next().unwrap().unwrap())
                }
            }
            (false, true) => {
                // Old has an old directory no longer present.
                let _ = self.left.next();
                self.state.push(CombineState::SameDirs);
                self.state.push(CombineState::LeftDirs);
                VisitResult::Continue
            }
            (true, false) => {
                // Directories present in new, not in old.
                self.state.push(CombineState::SameDirs);
                self.state.push(CombineState::RightDirs);
                vro!(self.right.next().unwrap().unwrap())
            }
        }
    }

    // Both trees are in the same directory, and we are looking at file
    // nodes.
    fn visit_samefiles(&mut self, left: &SureNode, right: &SureNode) -> VisitResult {
        debug!("visit samefiles: {:?}, {:?}", left, right);
        match (left.is_leave(), right.is_leave()) {
            (true, true) => {
                // Both are leaving at the same time, nothing to push onto
                // state.  Consume the nodes, and return the leave.
                let _ = self.left.next();
                vro!(self.right.next().unwrap().unwrap())
            }
            (true, false) => {
                self.state.push(CombineState::SameFiles);
                // New file added in new, not present in old.
                vro!(self.right.next().unwrap().unwrap())
            }
            (false, true) => {
                // File removed.
                self.state.push(CombineState::SameFiles);
                let _ = self.left.next();
                VisitResult::Continue
            },
            (false, false) => {
                self.state.push(CombineState::SameFiles);

                // Two names within a directory.
                if left.name() == right.name() {
                    // TODO: Here is where we copy the sha1 if there is a
                    // good match.
                    let left = self.left.next().unwrap().unwrap();
                    let mut right = self.right.next().unwrap().unwrap();
                    maybe_copy_sha(&left, &mut right);
                    vro!(right)
                } else if left.name() < right.name() {
                    // An old name no longer present.
                    let _ = self.left.next();
                    VisitResult::Continue
                } else {
                    // A new name with no corresponding old name.
                    vro!(self.right.next().unwrap().unwrap())
                }
            },
        }
    }

    fn visit_rightdirs(&mut self, left: &SureNode, right: &SureNode) -> VisitResult {
        debug!("visit rightdirs: {:?}, {:?}", left, right);
        if right.is_sep() {
            // Since we don't care about files, or matching, no need for
            // self.state.push(CombineState::RightFiles)
            // the RightFiles state, just stay.
            self.state.push(CombineState::RightDirs);
        } else if right.is_enter() {
            self.state.push(CombineState::RightDirs);
            self.state.push(CombineState::RightDirs);
        } else if right.is_leave() {
            // No state change.
        } else {
            // Otherwise, stays the same.
            self.state.push(CombineState::RightDirs);
        }
        vro!(self.right.next().unwrap().unwrap())
    }

    fn visit_leftdirs(&mut self, left: &SureNode, right: &SureNode) -> VisitResult {
        debug!("visit rightdirs: {:?}, {:?}", left, right);
        if left.is_sep() {
            // Since we don't care about files, or matching, no need for
            // self.state.push(CombineState::RightFiles)
            // the RightFiles state, just stay.
            self.state.push(CombineState::LeftDirs);
        } else if left.is_enter() {
            self.state.push(CombineState::LeftDirs);
            self.state.push(CombineState::LeftDirs);
        } else if left.is_leave() {
            // No state change.
        } else {
            // Otherwise, stays the same.
            self.state.push(CombineState::LeftDirs);
        }
        let _ = self.left.next();
        VisitResult::Continue
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
