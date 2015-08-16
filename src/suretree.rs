// SureTree

use ::Result;

use flate2::{self, Compression, FlateReadExt};
use std::collections::BTreeMap;
use std::os::unix::ffi::OsStringExt;
use std::fs::File;
use std::io::prelude::*;
use std::io::{self, BufReader, BufWriter};
use std::path::Path;

/// Represents a single directory entity.  The `String` values (name, or
/// att properties) are `Escape` encoded, when they represent a name in the
/// filesystem.
#[derive(Debug)]
pub struct SureTree {
    pub name: String,
    pub atts: BTreeMap<String, String>,
    pub children: Vec<SureTree>,
    pub files: Vec<SureFile>,
}

#[derive(Debug)]
pub struct SureFile {
    pub name: String,
    pub atts: BTreeMap<String, String>,
}

impl SureTree {
    pub fn load<P: AsRef<Path>>(name: P) -> Result<SureTree> {
        let rd = try!(File::open(name));
        let rd = try!(rd.gz_decode());
        let rd = BufReader::new(rd);
        let mut lines = rd.split('\n' as u8);

        try!(fixed(&mut lines, b"asure-2.0"));
        try!(fixed(&mut lines, b"-----"));

        let first = try!(Self::get_line(&mut lines));
        Self::subload(first, &mut lines)
    }

    fn subload<B: BufRead>(first: Vec<u8>, mut inp: &mut io::Split<B>) -> Result<SureTree>
    {
        let (name, atts) = decode_entity(&first[1..]);
        let mut children = vec![];

        let mut line = try!(Self::get_line(inp));
        loop {
            if line[0] != 'd' as u8 {
                break;
            }
            let tree = try!(Self::subload(line, &mut inp));
            children.push(tree);
            line = try!(Self::get_line(&mut inp));
        }

        if line != &['-' as u8] {
            return Err(From::from("surefile missing '-' marker'"));
        }

        let mut files = vec![];
        line = try!(Self::get_line(inp));
        loop {
            if line[0] != 'f' as u8 {
                break;
            }
            let (fname, fatts) = decode_entity(&line[1..]);
            files.push(SureFile { name: fname, atts: fatts });
            line = try!(Self::get_line(inp));
        }

        if line != &['u' as u8] {
            return Err(From::from("surefile missing 'u' marker'"));
        }

        Ok(SureTree {
            name: name,
            atts: atts,
            children: children,
            files: files,
        })
    }

    fn get_line<B: BufRead>(mut inp: &mut io::Split<B>) -> Result<Vec<u8>>
    {
        match inp.next() {
            None => return Err(From::from("surefile is truncated")),
            Some(l) => Ok(try!(l)),
        }
    }

    pub fn count_nodes(&self) -> usize {
        self.children.iter().fold(0, |acc, item| acc + item.count_nodes()) +
            self.files.len()
    }

    pub fn save<P: AsRef<Path>>(&self, name: P) -> Result<()> {
        let wr = try!(File::create(name));
        let wr = flate2::write::GzEncoder::new(wr, Compression::Default);
        let mut wr = BufWriter::new(wr);  // Benchmark with and without, gz might buffer.

        try!(writeln!(&mut wr, "asure-2.0"));
        try!(writeln!(&mut wr, "-----"));

        self.walk(&mut wr)
    }

    fn walk<W: Write>(&self, out: &mut W) -> Result<()> {
        try!(self.header(out, 'd', &self.name, &self.atts));
        for child in &self.children {
            try!(child.walk(out));
        }
        try!(writeln!(out, "-"));
        for child in &self.files {
            try!(self.header(out, 'f', &child.name, &child.atts));
        }
        try!(writeln!(out, "u"));
        Ok(())
    }

    fn header<W: Write>(&self, out: &mut W, kind: char, name: &str, atts: &BTreeMap<String, String>) -> Result<()> {
        try!(write!(out, "{}{} [", kind, name));

        // BTrees are sorted.
        for (k, v) in atts {
            try!(write!(out, "{} {} ", k, v));
        }
        try!(writeln!(out, "]"));
        Ok(())
    }
}

// TODO: These should return Result to handle errors.
fn decode_entity(text: &[u8]) -> (String, BTreeMap<String, String>) {
    let (name, mut text) = get_delim(text, ' ');
    trace!("name = '{:?}' ('{:?}')", name, String::from_utf8_lossy(&text));
    assert!(text[0] == '[' as u8);
    text = &text[1..];

    let mut atts = BTreeMap::new();
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
    (String::from_utf8(text[..space].to_owned()).unwrap(), &text[space + 1 ..])
}

fn fixed<I>(inp: &mut I, exp: &[u8]) -> Result<()>
    where I: Iterator<Item = io::Result<Vec<u8>>>
{
    match inp.next() {
        Some(Ok(ref text)) if &text[..] == exp => Ok(()),
        Some(Ok(ref text)) => Err(From::from(format!("Unexpected line: '{}', expect '{}'",
                                                     String::from_utf8_lossy(text),
                                                     String::from_utf8_lossy(exp)))),
        Some(Err(e)) => Err(From::from(format!("Error reading surefile: {}", e))),
        None => Err(From::from(format!("Unexpected eof on surefile"))),
    }
}
