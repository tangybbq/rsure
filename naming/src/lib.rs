//! A Naming manages a group of associated filenames.  All of these names
//! exist in a single directory, have a common basename, and various
//! suffixes.  It consists of the following names:
//!
//! *   path/base.dat.gz: The primary name
//! *   path/base.bak.gz: A backup file
//! *   path/base.0:      A temporary file
//! *   path/base.1.gz:   A compressed temporary file
//!
//! The client of this crate can determine with the primary and backup
//! names are compressed, and compression can be chosen for the temporary
//! files on a per-file basis.  If the compression matches the main name,
//! a temp file can be atomically renamed to the primary name.
//!
//! In addition to the management of the names, this module manages opening
//! and closing files associated with the names, as well as cleaning up
//! temporary files when the Naming goes out of scope.

use flate2::{write::GzEncoder, Compression};
use log::warn;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, ErrorKind, Write},
    path::{Path, PathBuf},
    result,
};

/// Our local Result type.  Makes use of `failure::Error` to automatically
/// pass errors upward.
type Result<T> = result::Result<T, failure::Error>;

#[derive(Debug)]
pub struct Naming {
    // The directory for files to be written to.
    path: PathBuf,
    // The base part of the filename
    base: String,
    // The extension to use for the main name.
    ext: String,
    // Are the primary and backup files to be compressed?
    compressed: bool,

    // Track the next temp we try to open, avoids O(n^2) open calls.  This
    // is merely an optimization and shouldn't have observable behavior.
    next_temp: usize,

    // The naming convention can be instructed to cleanup files when it is
    // dropped.
    cleanup: Vec<PathBuf>,
}

/// Something that can be written to, that remembers its name.  The writer
/// is boxed to support various kinds of writers, including compressed.
pub struct NamedWriter {
    pub name: PathBuf,
    pub writer: Box<dyn Write>,
}

impl Naming {
    pub fn new<P: AsRef<Path>>(path: P, base: &str, ext: &str, compressed: bool) -> Naming {
        Naming {
            path: path.as_ref().to_path_buf(),
            base: base.to_string(),
            ext: ext.to_string(),
            compressed: compressed,
            next_temp: 0,
            cleanup: Vec::new(),
        }
    }

    pub fn make_name(&self, ext: &str, compressed: bool) -> PathBuf {
        let name = format!(
            "{}.{}{}",
            self.base,
            ext,
            if compressed { ".gz" } else { "" }
        );
        self.path.join(name)
    }

    /// Construct a temp file that matches the given naming.
    pub fn temp_file(&mut self, compressed: bool) -> Result<(PathBuf, File)> {
        let mut n = self.next_temp;
        loop {
            let name = self.make_name(&n.to_string(), compressed);
            self.next_temp = n + 1;

            match OpenOptions::new().write(true).create_new(true).open(&name) {
                Ok(fd) => return Ok((name, fd)),
                Err(ref e) if e.kind() == ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.into()),
            }

            n += 1;
        }
    }

    /// Construct a temp file (as above), but if compression is requested,
    /// use a writer that compresses when writing.
    pub fn new_temp(&mut self, compressed: bool) -> Result<NamedWriter> {
        let (name, file) = self.temp_file(compressed)?;
        let writer = if compressed {
            // The GzEncoder does a measure of buffering.
            // TODO: Do benchmarks to determine if buffing the result of
            // the GzEncoder help.
            Box::new(GzEncoder::new(file, Compression::default())) as Box<dyn Write>
        } else {
            Box::new(BufWriter::new(file)) as Box<dyn Write>
        };
        Ok(NamedWriter {
            name: name,
            writer: writer,
        })
    }

    /// Replace the main file with the given name.  This attempts to rename
    /// the main name to the backup name, and then attempts to rename the
    /// temp file to the main name.
    pub fn rename_to_main(&self, name: &Path) -> Result<()> {
        let main_name = self.make_name(&self.ext, self.compressed);
        let back_name = self.make_name("bak", self.compressed);

        match fs::rename(&main_name, &back_name) {
            // Not found means there isn't a main name to rename.
            Err(ref e) if e.kind() == ErrorKind::NotFound => (),
            // Other errors are failure.
            Err(e) => return Err(e.into()),
            Ok(()) => (),
        }

        fs::rename(name, main_name)?;
        Ok(())
    }

    /// Add a name that must be cleaned up.
    pub fn add_cleanup(&mut self, name: PathBuf) {
        self.cleanup.push(name);
    }
}

impl Drop for Naming {
    fn drop(&mut self) {
        for name in &self.cleanup {
            if let Err(e) = fs::remove_file(name) {
                warn!("Error cleaning up: {:?} ({})", name, e);
            }
        }
    }
}
