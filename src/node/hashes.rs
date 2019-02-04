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
use log::error;
use rusqlite::{
    types::ToSql,
    Connection,
    NO_PARAMS,
};
use std::{
    io::Write,
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
