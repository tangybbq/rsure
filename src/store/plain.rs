// Plainfile storage of surefiles.

use ::Result;
use ::SureTree;
use flate2::{self, Compression, FlateReadExt};
use std::path::PathBuf;
use std::fs::{File, OpenOptions, rename};
use std::io::ErrorKind;
use super::{Store, StoreTags, Version};

pub struct Plain {
    pub path: PathBuf, // The directory where the surefiles will be written.
    pub base: String, // The initial part of the name, e.g. "2sure"
    pub compressed: bool, // Indicates the file should be compressed.
}

impl Plain {
    /// Construct a path name with the given extension.
    fn make_name(&self, ext: &str) -> PathBuf {
        let name = if self.compressed {
            format!("{}.{}.gz", self.base, ext)
        } else {
            format!("{}.{}", self.base, ext)
        };

        self.path.join(name)
    }

    /// Create a new temporary file for writing data.  The name will be unique to avoid any races.
    fn temp_file(&self) -> Result<(PathBuf, File)> {
        let mut n = 0;
        loop {
            let name = self.make_name(&n.to_string());

            match OpenOptions::new().write(true).create_new(true).open(&name) {
                Ok(fd) => return Ok((name, fd)),
                Err(ref e) if e.kind() == ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.into()),
            }

            n += 1;
        }
    }
}

impl Store for Plain {
    /// Write a new surefile out, archiving the previous version.
    fn write_new(&self, tree: &SureTree, _tags: &StoreTags) -> Result<()> {
        let tmp_name = {
            let (tmp_name, mut fd) = self.temp_file()?;
            if self.compressed {
                let wr = flate2::write::GzEncoder::new(fd, Compression::Default);
                tree.save_to(wr)?;
            } else {
                tree.save_to(&mut fd)?;
            }
            tmp_name
        };
        let dat_name = self.make_name("dat");
        let bak_name = self.make_name("bak");
        rename(&dat_name, &bak_name).unwrap_or(());
        rename(&tmp_name, &dat_name)?;
        Ok(())
    }

    /// Load a given surefile.
    fn load(&self, version: Version) -> Result<SureTree> {
        let ext = match version {
            Version::Latest => "dat",
            Version::Prior => "bak",
        };
        let name = self.make_name(ext);
        let rd = File::open(&name)?;
        if self.compressed {
            SureTree::load_from(rd.gz_decode()?)
        } else {
            SureTree::load_from(rd)
        }
    }
}
