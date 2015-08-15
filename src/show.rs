// Show module.

use ::suretree::SureTree;
use ::Result;

use flate2::FlateReadExt;
use flate2::read::GzDecoder;
use rustc_serialize::hex::FromHex;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::fs::File;
use std::io::prelude::*;
use std::io::{self, BufReader};

// TODO: Detect and correctly handle the ".dat.gz" suffix on the names.

#[derive(Debug)]
pub struct SureNode {
    pub name: OsString,
    pub atts: BTreeMap<String, OsString>,
}

impl SureNode {
    fn make(text: &[u8]) -> Result<SureNode> {
        let (name, atts) = decode_entity(text);
        Ok(SureNode {
            name: name,
            atts: atts,
        })
    }
}

#[derive(Debug)]
pub enum SureEntry {
    Dir(SureNode),
    Mark,
    Up,
    NonDir(SureNode),
}

pub struct SureFileIter {
    iter: io::Split<io::BufReader<GzDecoder<File>>>,
}

impl SureFileIter {
    pub fn open(name: &str) -> Result<SureFileIter> {
        let rd = try!(File::open(name));
        let rd = try!(rd.gz_decode());
        let rd = BufReader::new(rd);
        let mut iter = rd.split('\n' as u8);

        try!(fixed(&mut iter, b"asure-2.0"));
        try!(fixed(&mut iter, b"-----"));

        Ok(SureFileIter { iter: iter })
    }
}

impl Iterator for SureFileIter {
    type Item = Result<SureEntry>;

    fn next(&mut self) -> Option<Result<SureEntry>> {
        let line = match self.iter.next() {
            None => return None,
            Some(line) => match line {
                Ok(l) => l,
                Err(e) => return Some(Err(From::from(e))),
            }
        };

        if line.len() == 0 {
            return Some(Err(From::from("Blank line in surefile")));
        }

        match line[0] as char {
            'd' => {
                match SureNode::make(&line[1..]) {
                    Ok(n) => Some(Ok(SureEntry::Dir(n))),
                    Err(e) => Some(Err(e)),
                }
            },
            'f' => {
                match SureNode::make(&line[1..]) {
                    Ok(n) => Some(Ok(SureEntry::NonDir(n))),
                    Err(e) => Some(Err(e)),
                }
            },
            '-' => Some(Ok(SureEntry::Mark)),
            'u' => Some(Ok(SureEntry::Up)),
            ch => Some(Err(From::from(format!("Invalid leading character '{}' in surefile", ch)))),
        }
    }
}

pub fn show(name: &str) -> Result<()> {
    /*
    let mut count = 0;
    let mut elts = vec![];
    for ent in try!(SureFileIter::open(name)) {
        let ent = try!(ent);
        // println!("{:?}", ent);
        elts.push(ent);
        count += 1;
    }
    println!("count: {}", count);
    println!("count: {}", elts.len());
    */
    let tree = try!(SureTree::load(name));
    println!("{:#?}", tree);
    Ok(())
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
            buf.push(text[p] as u8);
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
