//! SCCS-style delta weave stores.

use crate::{
    node,
    store::{Store, StoreTags, StoreVersion, StoreWriter, TempCleaner, TempFile, TempLoader, Version},
    Result, SureNode, SureTree,
};
use failure::format_err;
use log::warn;
use std::{
    env,
    fs::{self, File},
    io::{self, BufRead, BufReader, Read, BufWriter, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
};
use weave::{self, DeltaWriter, NamingConvention, NewWeave, NullSink, Parser, SimpleNaming, Sink};

pub struct WeaveStore {
    naming: SimpleNaming,
}

impl WeaveStore {
    pub fn new<P: AsRef<Path>>(path: P, base: &str, compressed: bool) -> WeaveStore {
        WeaveStore {
            naming: SimpleNaming::new(path, base, "dat", compressed),
        }
    }
}

impl Store for WeaveStore {
    fn write_new(&self, tree: &SureTree, tags: &StoreTags) -> Result<()> {
        let itags = tags.iter().map(|(k, v)| (k.as_ref(), v.as_ref()));
        match weave::get_last_delta(&self.naming) {
            Ok(base) => {
                let mut wv = DeltaWriter::new(&self.naming, itags, base)?;
                tree.save_to(&mut wv)?;
                wv.close()?;
                Ok(())
            }
            Err(_) => {
                // Create a new weave file.
                let mut wv = NewWeave::new(&self.naming, itags)?;

                tree.save_to(&mut wv)?;

                wv.close()?;
                Ok(())
            }
        }
    }

    fn load(&self, version: Version) -> Result<SureTree> {
        let last = weave::get_last_delta(&self.naming)?;
        let last = match version {
            Version::Latest => last,
            Version::Prior => last - 1,
            Version::Tagged(vers) => vers.parse()?,
        };

        let child_naming = self.naming.clone();
        let (sender, receiver) = mpsc::sync_channel(32);
        let child = thread::spawn(move || {
            if let Err(err) = read_parse(&child_naming, last, sender.clone()) {
                // Attempt to send the last error over.
                if let Err(inner) = sender.send(Some(Err(err))) {
                    warn!("Error sending error on channel {:?}", inner);
                }
            }
        });
        let rd = ReadReceiver(receiver);
        let tree = SureTree::load_from(rd);
        match child.join() {
            Ok(()) => (),
            Err(e) => warn!("Problem joining child thread: {:?}", e),
        }
        tree
    }

    fn get_versions(&self) -> Result<Vec<StoreVersion>> {
        let header = Parser::new(&self.naming, NullSink, 1)?.into_header();
        let mut versions: Vec<_> = header
            .deltas
            .iter()
            .map(|v| StoreVersion {
                name: v.name.clone(),
                time: v.time,
                version: Version::Tagged(v.number.to_string()),
            }).collect();
        versions.reverse();
        Ok(versions)
    }

    fn load_iter(&self, version: Version) -> Result<Box<dyn Iterator<Item = Result<SureNode>>>> {
        let last = weave::get_last_delta(&self.naming)?;
        let last = match version {
            Version::Latest => last,
            Version::Prior => last - 1,
            Version::Tagged(vers) => vers.parse()?,
        };

        let child_naming = self.naming.clone();
        let (sender, receiver) = mpsc::sync_channel(32);
        let child = thread::spawn(move || {
            if let Err(err) = read_parse(&child_naming, last, sender.clone()) {
                // Attempt to send the last error over.
                if let Err(inner) = sender.send(Some(Err(err))) {
                    warn!("Error sending error on channel {:?}", inner);
                }
            }
        });

        fixed(&receiver, b"asure-2.0")?;
        fixed(&receiver, b"-----")?;

        Ok(Box::new(WeaveIter {
            _child: child,
            receiver: receiver,
        }))
    }

    fn make_temp(&self) -> Result<Box<dyn TempFile + '_>> {
        // TODO: Fixup naming to allow uncompressed writes.
        let (path, file) = self.naming.temp_file()?;
        let cpath = path.clone();
        Ok(Box::new(WeaveTemp {
            parent: self,
            path: path,
            file: BufWriter::new(file),
            cleaner: FileClean(cpath),
        }))
    }

    fn make_new(&self, tags: &StoreTags) -> Result<Box<dyn StoreWriter + '_>> {
        let itags = tags.iter().map(|(k, v)| (k.as_ref(), v.as_ref()));
        match weave::get_last_delta(&self.naming) {
            Ok(base) => {
                let wv = DeltaWriter::new(&self.naming, itags, base)?;
                Ok(Box::new(NewWeaveDelta { weave: wv }))
            }
            Err(_) => {
                // Create a new weave file.
                let wv = NewWeave::new(&self.naming, itags)?;
                Ok(Box::new(NewWeaveWriter { weave: wv }))
            }
        }
    }
}

struct WeaveTemp<'a> {
    parent: &'a WeaveStore,
    path: PathBuf,
    file: BufWriter<File>,
    cleaner: FileClean,
}

impl<'a> TempFile<'a> for WeaveTemp<'a> {
    fn into_loader(self: Box<Self>) -> Result<Box<dyn TempLoader + 'a>> {
        drop(self.file);
        Ok(Box::new(WeaveTempLoader {
            _parent: self.parent,
            path: self.path,
            cleaner: self.cleaner,
        }))
    }

    fn into_cleaner(self: Box<Self>) -> Result<Box<dyn TempCleaner>> {
        Ok(Box::new(self.cleaner))
    }
}

impl<'a> Write for WeaveTemp<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

pub struct WeaveTempLoader<'a> {
    _parent: &'a WeaveStore,
    path: PathBuf,
    cleaner: FileClean,
}

impl<'a> TempLoader for WeaveTempLoader<'a> {
    fn new_loader(&self) -> Result<Box<dyn BufRead>> {
        let read = BufReader::new(File::open(&self.path)?);
        Ok(Box::new(read))
    }

    fn path_ref(&self) -> &Path {
        &self.path
    }

    fn into_cleaner(self: Box<Self>) -> Result<Box<dyn TempCleaner>> {
        Ok(Box::new(self.cleaner))
    }
}

pub struct NewWeaveWriter<'a> {
    weave: NewWeave<'a>,
}

impl<'a> StoreWriter<'a> for NewWeaveWriter<'a> {
    fn commit(self: Box<Self>) -> Result<()> {
        self.weave.close()
    }
}

impl<'a> Write for NewWeaveWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.weave.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.weave.flush()
    }
}

pub struct NewWeaveDelta<'a> {
    weave: DeltaWriter<'a>,
}

impl<'a> StoreWriter<'a> for NewWeaveDelta<'a> {
    fn commit(self: Box<Self>) -> Result<()> {
        self.weave.close()
    }
}

impl<'a> Write for NewWeaveDelta<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.weave.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.weave.flush()
    }
}

pub struct WeaveIter {
    // This field doesn't need to be referenced, just held onto, the Drop
    // implementation for it is sufficient cleanup.
    _child: JoinHandle<()>,
    receiver: Receiver<Option<Result<String>>>,
}

impl Iterator for WeaveIter {
    type Item = Result<SureNode>;

    fn next(&mut self) -> Option<Result<SureNode>> {
        let line = match self.receiver.recv() {
            Ok(None) => return None,
            Ok(Some(Err(e))) => return Some(Err(e)),
            Ok(Some(Ok(line))) => line,
            Err(e) => return Some(Err(e.into())),
        };
        let line = line.as_bytes();

        match line[0] {
            b'd' => {
                let (dname, datts) = node::decode_entity(&line[1..]);
                Some(Ok(SureNode::Enter{name: dname, atts: datts}))
            }
            b'f' => {
                let (fname, fatts) = node::decode_entity(&line[1..]);
                Some(Ok(SureNode::File{name: fname, atts: fatts}))
            }
            b'-' => Some(Ok(SureNode::Sep)),
            b'u' => Some(Ok(SureNode::Leave)),
            ch => Some(Err(format_err!("Invalid surefile line start: {:?}", ch)))
        }
    }
}

// Parse a given delta, emitting the lines to the given channel.  Finishes with Ok(()), or an error
// if something goes wrong.
fn read_parse(
    naming: &dyn NamingConvention,
    delta: usize,
    chan: SyncSender<Option<Result<String>>>,
) -> Result<()> {
    let mut parser = Parser::new(naming, ReadSync { chan: chan }, delta)?;
    parser.parse_to(0)?;
    let sink = parser.get_sink();
    match sink.borrow().chan.send(None) {
        Ok(()) => (),
        Err(e) => return Err(format_err!("chan send error: {:?}", e)),
    }
    Ok(())
}

/// Try reading a specific line from the given channel.  Returns Err if the
/// line didn't match, or something went wrong with the read.
fn fixed(recv: &Receiver<Option<Result<String>>>, expect: &[u8]) -> Result<()> {
    match recv.recv() {
        Ok(Some(Ok(line))) => {
            if line.as_bytes() == expect {
                Ok(())
            } else {
                Err(format_err!("Unexpect line from channel: {:?} expect {:?}", line, expect))
            }
        }
        Ok(Some(Err(e))) => Err(format_err!("Error reading suredata: {:?}", e)),
        Ok(None) => Err(format_err!("Unexpected eof reading suredata")),
        Err(e) => Err(e.into()),
    }
}

struct ReadSync {
    chan: SyncSender<Option<Result<String>>>,
}

impl Sink for ReadSync {
    fn plain(&mut self, text: &str, keep: bool) -> weave::Result<()> {
        if keep {
            match self.chan.send(Some(Ok(text.to_string()))) {
                Ok(()) => Ok(()),
                Err(e) => Err(format_err!("chan send error: {:?}", e)),
            }
        } else {
            Ok(())
        }
    }
}

struct ReadReceiver(Receiver<Option<Result<String>>>);

impl Read for ReadReceiver {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let line = match self.0.recv() {
            Ok(line) => line,
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("channel error: {:?}", e),
                ))
            }
        };
        let line = match line {
            None => return Ok(0),
            Some(Ok(line)) => line,
            Some(Err(e)) => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("channel error: {:?}", e),
                ))
            }
        };
        let chars = line.as_bytes();
        if chars.len() + 1 > buf.len() {
            panic!("TODO: Handle line longer than buffer");
        }
        buf[..chars.len()].copy_from_slice(chars);
        buf[chars.len()] = b'\n';
        Ok(chars.len() + 1)
    }
}

/// Own a PathBuf, and delete this file on drop.  This is in its own type
/// for two reason. 1. I makes it easy to have cleaning in multiple types,
/// passing ownership between them, and 2.  It prevents the need for those
/// types to implement drop, which prevents moves out of the fields.
struct FileClean(PathBuf);

impl Drop for FileClean {
    fn drop(&mut self) {
        if env::var_os("RSURE_KEEP").is_none() {
            let _ = fs::remove_file(&self.0);
        }
    }
}

impl TempCleaner for FileClean{}
