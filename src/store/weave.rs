//! SCCS-style delta weave stores.

use crate::{
    Error,
    node,
    store::{Store, StoreTags, StoreVersion, StoreWriter, TempCleaner, TempFile, TempLoader, Version},
    Result, SureNode,
};
use std::{
    env,
    fs::{self, File},
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};
use weave::{self, DeltaWriter, NamingConvention, NewWeave, NullSink, Parser, PullParser, SimpleNaming};

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

        Ok(Box::new(WeaveIter::new(&self.naming, last)?))
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
        self.weave.close()?;
        Ok(())
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
        self.weave.close()?;
        Ok(())
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
    pull: Box<dyn Iterator<Item = Result<String>>>,
}

impl WeaveIter {
    fn new(naming: &dyn NamingConvention, delta: usize) -> Result<WeaveIter> {
        let mut pull = PullParser::new(naming, delta)?.filter_map(kept_text);
        fixed(&mut pull, "asure-2.0")?;
        fixed(&mut pull, "-----")?;
        Ok(WeaveIter { pull: Box::new(pull) })
    }
}

impl Iterator for WeaveIter {
    type Item = Result<SureNode>;

    fn next(&mut self) -> Option<Result<SureNode>> {
        let line = match self.pull.next() {
            Some(Err(e)) => return Some(Err(e)),
            Some(Ok(line)) => line,
            None => return None,
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
            ch => Some(Err(Error::InvalidSurefileChar(ch as char))),
        }
    }
}

// Filter nodes to only include kept text lines.
fn kept_text(node: weave::Result<weave::Entry>) -> Option<Result<String>> {
    match node {
        Err(e) => Some(Err(e.into())),
        Ok(weave::Entry::Plain { text, keep }) if keep => Some(Ok(text)),
        _ => None,
    }
}

/// Try reading a specific line from the given iterator.  Returns Err if
/// the line didn't match, or something went wrong with the read.
fn fixed<I>(pull: &mut I, expect: &str) -> Result<()>
    where I: Iterator<Item = Result<String>>
{
    match pull.next() {
        Some(Ok(line)) => {
            if line == expect {
                Ok(())
            } else {
                Err(Error::UnexpectedLine(line.into(), expect.into()))
            }
        }
        Some(Err(e)) => Err(e),
        None => Err(Error::SureFileEof),
    }
}
/*
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
*/

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
