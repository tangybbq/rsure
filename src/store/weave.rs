//! SCCS-style delta weave stores.

use Result;
use SureTree;
use std::io::{self, Read};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use super::{Store, StoreTags, StoreVersion, Version};
use weave::{self, Parser, DeltaWriter, SimpleNaming, NamingConvention, NewWeave, NullSink, Sink};

pub struct WeaveStore {
    naming: SimpleNaming,
}

impl WeaveStore {
    pub fn new<P: AsRef<Path>>(path: P, base: &str, compressed: bool) -> WeaveStore {
        WeaveStore { naming: SimpleNaming::new(path, base, "weave", compressed) }
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
        let (sender, receiver) = mpsc::channel();
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
            .map(|v| {
                StoreVersion {
                    name: v.name.clone(),
                    time: v.time,
                    version: Version::Tagged(v.number.to_string()),
                }
            })
            .collect();
        versions.reverse();
        Ok(versions)
    }
}

// Parse a given delta, emitting the lines to the given channel.  Finishes with Ok(()), or an error
// if something goes wrong.
fn read_parse(
    naming: &NamingConvention,
    delta: usize,
    chan: Sender<Option<Result<String>>>,
) -> Result<()> {
    let mut parser = Parser::new(naming, ReadSync { chan: chan }, delta)?;
    parser.parse_to(0)?;
    let sink = parser.get_sink();
    match sink.borrow().chan.send(None) {
        Ok(()) => (),
        Err(e) => return Err(format!("chan send error: {:?}", e).into()),
    }
    Ok(())
}

struct ReadSync {
    chan: Sender<Option<Result<String>>>,
}

impl Sink for ReadSync {
    fn plain(&mut self, text: &str, keep: bool) -> weave::Result<()> {
        if keep {
            match self.chan.send(Some(Ok(text.to_string()))) {
                Ok(()) => Ok(()),
                Err(e) => Err(format!("chan send error: {:?}", e).into()),
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
