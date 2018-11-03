//! Implement Weave deltas, inspired by SCCS.
//!
//! Although not much remains of the SCCS revision control system, it's "weave" delta format turns
//! out to be a good way of representing multiple versions of data that differ only in parts.
//!
//! This package implements a weave-based storage of "plain text", where plain text consists of
//! lines of UTF-8 printable characters separated by a newline.
//!
//! The format is similar to SCCS, but with no constraints to keep what are relatively poor design
//! decisions from SCCS, such as putting a checksum at the top of the file, and using limited-sized
//! field for values such as the number of lines in a file, or the use of 2-digit years.  However,
//! the main body of the weaved file, that which describes inserts and deletes is the same, and
//! allows us to test this version by comparing with the storage of sccs.
//!
//! Writing an initial weave works as a regular file writer.  The file itself has a small amount of
//! surrounding meta-data, but is otherwise mostly just the contents of the initial file.
//!
//! Adding a delta to a weave file requires extracting a base version that the delta will be made
//! against (the base does not need to be the tip version, allowing for branches).  This crate will
//! need to make several temporary files.

#![warn(bare_trait_objects)]

#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;
extern crate chrono;
extern crate flate2;
extern crate regex;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

mod errors;
mod naming;
mod parse;
mod newweave;
mod delta;
mod header;

pub use crate::naming::NamingConvention;
pub use crate::naming::SimpleNaming;
pub use crate::errors::{Result, Error};
pub use crate::parse::{Sink, Parser};
pub use crate::newweave::NewWeave;
pub use crate::delta::DeltaWriter;
pub use crate::header::{Header, DeltaInfo};

use std::io::Write;
use std::path::PathBuf;

/// Something we can write into, that remembers its name.  The writer is boxed because the writer
/// may be compressed.
pub struct WriterInfo {
    name: PathBuf,
    writer: Box<dyn Write>,
}

/// Read the header from a weave file.
pub fn read_header(naming: &dyn NamingConvention) -> Result<Header> {
    Ok(Parser::new(naming, NullSink, 1)?.into_header())
}

/// Retrieve the last delta in the weave file.  Will panic if the weave file is malformed and
/// contains no revisions.
pub fn get_last_delta(naming: &dyn NamingConvention) -> Result<usize> {
    let header = read_header(naming)?;
    Ok(header.deltas.iter().map(|x| x.number).max().expect(
        "at least one delta in weave file",
    ))
}

/// A null sink that does nothing, useful for parsing the header.
pub struct NullSink;

impl Sink for NullSink {}
