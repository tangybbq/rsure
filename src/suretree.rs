// SureTree

use ::Result;

use flate2::FlateReadExt;
use rustc_serialize::hex::FromHex;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::fs::File;
use std::io::prelude::*;
use std::io::{self, BufReader};

// Unfortunately, this is very wasteful of memory, so isn't really a
// practical way of doing this.  We'll probably still need to do this via
// streaming.
#[derive(Debug)]
pub struct SureTree {
    pub name: OsString,
    pub atts: BTreeMap<String, OsString>,
    pub children: Vec<SureTree>,
    pub files: Vec<SureFile>,
}

#[derive(Debug)]
pub struct SureFile {
    pub name: OsString,
    pub atts: BTreeMap<String, OsString>,
}

impl SureTree {
    pub fn load(name: &str) -> Result<SureTree> {
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
}

// TODO: These should return Result to handle errors.
fn decode_entity(text: &[u8]) -> (OsString, BTreeMap<String, OsString>) {
    let (name, mut text) = get_delim(text, ' ');
    let name = to_osstring(name);
    trace!("name = '{:?}' ('{:?}')", name, String::from_utf8_lossy(&text));
    assert!(text[0] == '[' as u8);
    text = &text[1..];

    let mut atts = BTreeMap::new();
    while text[0] != ']' as u8 {
        let (key, t2) = get_delim(text, ' ');
        let (value, t2) = get_delim(t2, ' ');
        trace!("  {} = {}", String::from_utf8_lossy(&key), String::from_utf8_lossy(&value));
        text = t2;

        atts.insert(String::from_utf8(key.to_owned()).unwrap(), to_osstring(value));
    }

    (name, atts)
}

fn to_osstring(text: &[u8]) -> OsString {
    let mut buf = Vec::with_capacity(text.len());
    let mut p = 0;
    while p < text.len() {
        if text[p] == '=' as u8 {
            let num = String::from_utf8(text[p+1 .. p+3].to_owned()).unwrap();
            let num = num.from_hex().unwrap();
            assert!(num.len() == 1);
            buf.push(num[0]);
            p += 2;
        } else {
            buf.push(text[p]);
        }
        p += 1;
    }

    OsStringExt::from_vec(buf)
}

fn get_delim(text: &[u8], delim: char) -> (&[u8], &[u8]) {
    let mut it = text.iter();
    let space = it.position(|&s| s == delim as u8).unwrap();
    (&text[..space], &text[space + 1 ..])
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
