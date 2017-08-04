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

extern crate error_chain;
#[macro_use]
extern crate derive_error_chain;
#[macro_use]
extern crate log;

mod errors;
mod naming;
mod parse;

pub use naming::NamingConvention;
pub use naming::{SimpleNaming};
pub use errors::{Result, Error, ErrorKind};
pub use parse::{Sink, Parser};

use std::fs::{File, rename};
use std::mem::replace;
use std::io::{self, Write};
use std::path::PathBuf;

/// A builder for a new weave file.  The data should be written as a writer.  Closing the weaver
/// will finish up the write and move the new file into place.  If the weaver is just dropped, the
/// file will not be moved into place.
pub struct NewWeave<'n> {
    naming: &'n NamingConvention,
    temp: Option<WriterInfo>,
}

struct WriterInfo {
    name: PathBuf,
    file: File,
}

impl<'n> NewWeave<'n> {
    pub fn new<'a, 'b, I>(nc: &NamingConvention, tags: I) -> Result<NewWeave>
        where I: Iterator<Item=(&'a str, &'b str)>
    {
        let (temp, mut file) = nc.temp_file()?;
        for (k, v) in tags {
            writeln!(&mut file, "\x01t {}={}", k, v)?;
        }
        writeln!(&mut file, "\x01I 1")?;

        Ok(NewWeave {
            naming: nc,
            temp: Some(WriterInfo {
                name: temp,
                file: file,
            }),
        })
    }

    pub fn close(mut self) -> Result<()> {
        let temp = replace(&mut self.temp, None);
        let name = match temp {
            Some(mut wi) => {
                writeln!(&mut wi.file, "\x01E 1")?;
                wi.name
            }
            None => return Err("NewWeave already closed".into()),
        };
        let _ = rename(self.naming.main_file(), self.naming.backup_file());
        rename(name, self.naming.main_file())?;
        Ok(())
    }
}

impl<'n> Write for NewWeave<'n> {
    // Write the data out, just passing it through to the underlying file write.  We assume the
    // last line is terminated, or the resulting weave will be invalid.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.temp.as_mut()
            .expect("Attempt to write to NewWeave that is closed")
            .file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.temp.as_mut()
            .expect("Attempt to flush NewWeave that is closed")
            .file.flush()
    }
}
