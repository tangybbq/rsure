// SureTree

use crate::Result;

use failure::{err_msg, format_err};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use log::{log, trace};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::prelude::*;
use std::io::{self, BufReader, BufWriter};
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use super::escape::*;

pub type AttMap = BTreeMap<String, String>;

/// Represents a single directory entity.  The `String` values (name, or
/// att properties) are `Escape` encoded, when they represent a name in the
/// filesystem.
#[derive(Debug)]
pub struct SureTree {
    pub name: String,
    pub atts: AttMap,
    pub children: Vec<SureTree>,
    pub files: Vec<SureFile>,
}

#[derive(Debug)]
pub struct SureFile {
    pub name: String,
    pub atts: AttMap,
}

impl SureTree {
    /// Load a sure tree from a standard gzip compressed surefile.
    pub fn load<P: AsRef<Path>>(name: P) -> Result<SureTree> {
        let rd = File::open(name)?;
        let rd = GzDecoder::new(rd);
        Self::load_from(rd)
    }

    /// Load a sure tree from the given reader.
    pub fn load_from<R: Read>(rd: R) -> Result<SureTree> {
        let rd = BufReader::new(rd);
        let mut lines = rd.split('\n' as u8);

        fixed(&mut lines, b"asure-2.0")?;
        fixed(&mut lines, b"-----")?;

        let first = Self::get_line(&mut lines)?;
        Self::subload(first, &mut lines)
    }

    fn subload<B: BufRead>(first: Vec<u8>, mut inp: &mut io::Split<B>) -> Result<SureTree> {
        let (name, atts) = decode_entity(&first[1..]);
        let mut children = vec![];

        let mut line = Self::get_line(inp)?;
        loop {
            if line[0] != 'd' as u8 {
                break;
            }
            let tree = Self::subload(line, &mut inp)?;
            children.push(tree);
            line = Self::get_line(&mut inp)?;
        }

        if line != &['-' as u8] {
            return Err(err_msg("surefile missing '-' marker'"));
        }

        let mut files = vec![];
        line = Self::get_line(inp)?;
        loop {
            if line[0] != 'f' as u8 {
                break;
            }
            let (fname, fatts) = decode_entity(&line[1..]);
            files.push(SureFile {
                name: fname,
                atts: fatts,
            });
            line = Self::get_line(inp)?;
        }

        if line != &['u' as u8] {
            return Err(err_msg("surefile missing 'u' marker'"));
        }

        Ok(SureTree {
            name: name,
            atts: atts,
            children: children,
            files: files,
        })
    }

    fn get_line<B: BufRead>(inp: &mut io::Split<B>) -> Result<Vec<u8>> {
        match inp.next() {
            None => return Err(err_msg("surefile is truncated")),
            Some(l) => Ok(l?),
        }
    }

    pub fn count_nodes(&self) -> usize {
        self.children
            .iter()
            .fold(0, |acc, item| acc + item.count_nodes())
            + self.files.len()
    }

    /// Write a sure tree to a standard gzipped file of the given name.
    pub fn save<P: AsRef<Path>>(&self, name: P) -> Result<()> {
        let wr = File::create(name)?;
        let wr = GzEncoder::new(wr, Compression::default());
        self.save_to(wr)
    }

    /// Write a sure tree to the given writer.
    pub fn save_to<W: Write>(&self, wr: W) -> Result<()> {
        let mut wr = BufWriter::new(wr); // Benchmark with and without, gz might buffer.

        writeln!(&mut wr, "asure-2.0")?;
        writeln!(&mut wr, "-----")?;

        self.walk(&mut wr)
    }

    fn walk<W: Write>(&self, out: &mut W) -> Result<()> {
        self.header(out, 'd', &self.name, &self.atts)?;
        for child in &self.children {
            child.walk(out)?;
        }
        writeln!(out, "-")?;
        for child in &self.files {
            self.header(out, 'f', &child.name, &child.atts)?;
        }
        writeln!(out, "u")?;
        Ok(())
    }

    fn header<W: Write>(&self, out: &mut W, kind: char, name: &str, atts: &AttMap) -> Result<()> {
        write!(out, "{}{} [", kind, name)?;

        // BTrees are sorted.
        for (k, v) in atts {
            write!(out, "{} {} ", k, v)?;
        }
        writeln!(out, "]")?;
        Ok(())
    }
}

// TODO: These should return Result to handle errors.
fn decode_entity(text: &[u8]) -> (String, AttMap) {
    let (name, mut text) = get_delim(text, ' ');
    trace!(
        "name = '{:?}' ('{:?}')",
        name,
        String::from_utf8_lossy(&text)
    );
    assert!(text[0] == '[' as u8);
    text = &text[1..];

    let mut atts = AttMap::new();
    while text[0] != ']' as u8 {
        let (key, t2) = get_delim(text, ' ');
        let (value, t2) = get_delim(t2, ' ');
        trace!("  {} = {}", key, value);
        text = t2;

        atts.insert(key, value);
    }

    (name, atts)
}

fn get_delim(text: &[u8], delim: char) -> (String, &[u8]) {
    let mut it = text.iter();
    let space = it.position(|&s| s == delim as u8).unwrap();
    (
        String::from_utf8(text[..space].to_owned()).unwrap(),
        &text[space + 1..],
    )
}

fn fixed<I>(inp: &mut I, exp: &[u8]) -> Result<()>
where
    I: Iterator<Item = io::Result<Vec<u8>>>,
{
    match inp.next() {
        Some(Ok(ref text)) if &text[..] == exp => Ok(()),
        Some(Ok(ref text)) => Err(format_err!(
            "Unexpected line: '{}', expect '{}'",
            String::from_utf8_lossy(text),
            String::from_utf8_lossy(exp)
        )),
        Some(Err(e)) => Err(format_err!("Error reading surefile: {}", e)),
        None => Err(err_msg("Unexpected eof on surefile")),
    }
}

/// Files and trees both have names.  These names are escaped.
pub trait Named {
    // Return the escaped name of this entity.
    fn get_name(&self) -> &str;
}

impl Named for SureTree {
    fn get_name(&self) -> &str {
        &self.name
    }
}

impl Named for SureFile {
    fn get_name(&self) -> &str {
        &self.name
    }
}

/// Tree and file nodes can add themselves to a path.
pub trait PathAdd {
    /// Given an existing path, add the component of this entity to that
    /// path, and return the resulting PathBuf.
    fn join(&self, path: &Path) -> PathBuf;
}

impl<T: Named> PathAdd for T {
    fn join(&self, path: &Path) -> PathBuf {
        let s: OsString = OsStringExt::from_vec(self.get_name().unescape().unwrap());
        path.join(&s)
    }
}

// Provide for strings as well, assuming they are also escaped.
impl PathAdd for str {
    fn join(&self, path: &Path) -> PathBuf {
        let s: OsString = OsStringExt::from_vec(self.unescape().unwrap());
        path.join(&s)
    }
}
