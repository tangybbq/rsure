//! Weave deltas, inspired by SCCS.
//!
//! The [SCCS](https://en.wikipedia.org/wiki/Source_Code_Control_System) revision control system is
//! one of the oldest source code management systems (1973).  Although many of its concepts are
//! quite dated in these days of git, the underlying "weave" delta format it used turns out to be a
//! good way of representing multiple versions of data that differ only in parts.
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
//! Weave files are written using [`NewWeave`], which works like a regular file writer.  The file
//! itself has a small amount of surrounding metadata, but is otherwise mostly just the contents of
//! the initial file.
//!
//! Adding a delta to a weave file is done with the [`DeltaWriter`].  This is also written to, as a
//! regular file, and then [`DeltaWriter::close`] method will extract a base revision and use the
//! `diff` command to write a new version of the weave.  The `close` method will make several
//! temporary files in the process.
//!
//! The weave data is stored using a [`NamingConvention`], a trait that manages a related
//! collection of files, and temp files.  [`SimpleNaming`] is a basic representation of this that
//! has a base name, a backup file, and some temporary files.  The data in the file can be
//! compressed.

#![warn(bare_trait_objects)]

mod delta;
mod errors;
mod header;
mod naming;
mod newweave;
mod parse;

pub use crate::{
    delta::DeltaWriter,
    errors::{Error, Result},
    header::{DeltaInfo, Header},
    naming::NamingConvention,
    naming::SimpleNaming,
    naming::Compression,
    newweave::NewWeave,
    parse::{Entry, Parser, PullParser, Sink},
};

use std::{io::Write, path::PathBuf};

/// Something we can write into, that remembers its name.  The writer is boxed because the writer
/// may be compressed.
pub struct WriterInfo {
    name: PathBuf,
    writer: Box<dyn Write>,
}

/// Read the header from a weave file.
pub fn read_header(naming: &dyn NamingConvention) -> Result<Header> {
    Ok(PullParser::new(naming, 1)?.into_header())
}

/// Retrieve the last delta in the weave file.  Will panic if the weave file is malformed and
/// contains no revisions.
pub fn get_last_delta(naming: &dyn NamingConvention) -> Result<usize> {
    let header = read_header(naming)?;
    Ok(header
        .deltas
        .iter()
        .map(|x| x.number)
        .max()
        .expect("at least one delta in weave file"))
}
