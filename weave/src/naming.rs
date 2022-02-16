//! Weave files will follow a file naming convention.  This determines the names of various temp
//! files and other aspects.  The SCCS conventions are not followed, because they are not safe
//! (this crate will never write to a file that already exists).

use crate::{Result, WriterInfo};
use flate2::write::GzEncoder;
use std::{
    fs::{File, OpenOptions},
    io::{BufWriter, ErrorKind, Write},
    path::{Path, PathBuf},
};

/// A naming convention provides utilities needed to find the involved files, and construct
/// temporary files as part of writing the new weave.  The underlying object should keep the path
/// and base name.
///
/// The main file is either used by name, or opened for reading.  It should never be written to
/// directly.  The main file is always compressed if the convention enables compression.
///
/// The backup file is only used by name.  It is neither written to, nor read.  It will be
/// compressed, as it always comes from renaming the main file.
///
/// The temporary files are used by name, and written to.  They may or may not be compressed,
/// depending on how they will be used.
pub trait NamingConvention {
    /// Create a temporary file for writing.  Upon success, returns the full path of the file, and
    /// the opened File for writing to the file.  The path should refer to a new file that did not
    /// exist prior to this call.
    fn temp_file(&self) -> Result<(PathBuf, File)>;

    /// Return the pathname of the primary file.
    fn main_file(&self) -> PathBuf;

    /// Return the pathname of the backup file.
    fn backup_file(&self) -> PathBuf;

    /// Return if compression is requested on main file.
    fn compression(&self) -> Compression;

    /// Open a possibly compressed temp file, returning a WriterInfo for it.  The stream will be
    /// buffered, and possibly compressed.
    fn new_temp(&self) -> Result<WriterInfo> {
        let (name, file) = self.temp_file()?;
        let writer = match self.compression() {
            Compression::Plain =>
                Box::new(BufWriter::new(file)) as Box<dyn Write>,
            Compression::Gzip =>
                Box::new(GzEncoder::new(file, flate2::Compression::default())) as Box<dyn Write>,
        };
        Ok(WriterInfo { name, writer })
    }
}

/// Supported compression types.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Compression {
    Plain,
    Gzip,
}

/// The SimpleNaming is a NamingConvention that has a basename, with the main file having a
/// specified extension, the backup file having a ".bak" extension, and the temp files using a
/// numbered extension starting with ".0".  If the names are intended to be compressed, a ".gz"
/// suffix can also be added.
#[derive(Debug, Clone)]
pub struct SimpleNaming {
    // The directory for the files to be written.
    path: PathBuf,
    // The string for the base filename.
    base: String,
    // The extension to use for the main name.
    ext: String,
    // Compression to be used.
    compression: Compression,
}

impl SimpleNaming {
    pub fn new<P: AsRef<Path>>(path: P, base: &str, ext: &str, compression: Compression) -> SimpleNaming {
        SimpleNaming {
            path: path.as_ref().to_path_buf(),
            base: base.to_string(),
            ext: ext.to_string(),
            compression,
        }
    }

    pub fn make_name(&self, ext: &str, compression: Compression) -> PathBuf {
        let name = format!(
            "{}.{}{}",
            self.base,
            ext,
            match compression {
                Compression::Plain => "",
                Compression::Gzip => ".gz",
            },
        );
        self.path.join(name)
    }
}

impl NamingConvention for SimpleNaming {
    fn main_file(&self) -> PathBuf {
        self.make_name(&self.ext, self.compression)
    }

    fn backup_file(&self) -> PathBuf {
        self.make_name("bak", self.compression)
    }

    fn temp_file(&self) -> Result<(PathBuf, File)> {
        let mut n = 0;
        loop {
            let name = self.make_name(&n.to_string(), Compression::Plain);

            match OpenOptions::new().write(true).create_new(true).open(&name) {
                Ok(fd) => return Ok((name, fd)),
                Err(ref e) if e.kind() == ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.into()),
            }

            n += 1;
        }
    }

    fn compression(&self) -> Compression {
        self.compression
    }
}
