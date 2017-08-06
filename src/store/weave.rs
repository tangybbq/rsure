//! SCCS-style delta weave stores.

use Result;
use SureTree;
use std::cell::RefCell;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use super::{Store, StoreTags, Version};
use weave::{self, DeltaWriter, SimpleNaming, NamingConvention, NewWeave, Parser, Sink};

pub struct WeaveStore {
    naming: SimpleNaming,
}

impl WeaveStore {
    pub fn new<P: AsRef<Path>>(path: P, base: &str, compressed: bool) -> WeaveStore {
        WeaveStore {
            naming: SimpleNaming::new(path, base, "weave", compressed),
        }
    }
}

impl Store for WeaveStore {
    fn write_new(&self, tree: &SureTree, tags: &StoreTags) -> Result<()> {
        let itags = tags.iter().map(|(k,v)| (k.as_ref(), v.as_ref()));
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
        };

        let path = self.naming.main_file().to_path_buf();
        let (sender, receiver) = mpsc::channel();
        let child = thread::spawn(move || {
            if let Err(err) = read_parse(path, last, sender.clone()) {
                // Attempt to send the last error over.
                if let Err(inner) = sender.send(Some(Err(err))) {
                    warn!("Error sending error on channel {:?}",
                        inner);
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
}

// Parse a given delta, emitting the lines to the given channel.  Finishes with Ok(()), or an error
// if something goes wrong.
fn read_parse(path: PathBuf, delta: usize, chan: Sender<Option<Result<String>>>) -> Result<()> {
    let fd = File::open(&path)?;
    let lines = BufReader::new(fd).lines();
    let sync = Rc::new(RefCell::new(ReadSync {
        chan: chan,
    }));
    let mut parser = Parser::new(lines, sync.clone(), delta)?;
    parser.parse_to(0)?;
    match sync.borrow().chan.send(None) {
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
            Err(e) => return Err(io::Error::new(io::ErrorKind::Other, format!("channel error: {:?}", e))),
        };
        let line = match line {
            None => return Ok(0),
            Some(Ok(line)) => line,
            Some(Err(e)) =>
                return Err(io::Error::new(io::ErrorKind::Other, format!("channel error: {:?}", e))),
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
