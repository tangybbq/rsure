/// Computing hashes for files.

use std::ffi::OsString;
use std::io::prelude::*;
use std::os::unix::ffi::OsStringExt;
use std::path::Path;

use openssl::hash::{Hasher, MessageDigest};

use rustc_serialize::hex::ToHex;

use super::Result;
use super::suretree::{SureFile, SureTree};
use super::escape::*;
use super::progress::Progress;

pub trait SureHash {
    /// Estimate how much work (files and bytes) need to be hashed.
    fn hash_estimate(&self) -> Estimate;

    /// Update the hashes on any files that are missing them.
    /// Note that this only logs errors, and tries to continue.
    fn hash_update(&mut self, path: &Path, meter: &mut Progress);
}

#[derive(Debug)]
pub struct Estimate {
    pub files: u64,
    pub bytes: u64,
}

impl SureHash for SureTree {
    fn hash_estimate(&self) -> Estimate {
        let mut est = Estimate { files: 0, bytes: 0 };
        est.update(self);
        est
    }

    fn hash_update(&mut self, path: &Path, meter: &mut Progress) {
        for d in &mut self.children {
            let s: OsString = OsStringExt::from_vec(d.name.unescape().unwrap());
            let cpath = path.join(&s);
            d.hash_update(&cpath, meter);
        }

        for f in &mut self.files {
            if !f.needs_hash() {
                continue;
            }

            let s: OsString = OsStringExt::from_vec(f.name.unescape().unwrap());
            let fpath = path.join(&s);

            match noatime_open(&fpath) {
                Ok(mut fd) => {
                    match hash_file(&mut fd) {
                        Ok(h) => {
                            let hex = h.to_hex();
                            f.atts.insert("sha1".to_string(), hex);
                        }
                        Err(e) => {
                            error!("Unable to has file: '{:?}' ({})", fpath, e);
                        }
                    }
                }
                Err(e) => {
                    error!("Unable to open '{:?}' for hashing ({})", fpath, e);
                }
            }

            meter.update(1, f.atts["size"].parse().unwrap());
        }
    }
}

impl SureFile {
    fn needs_hash(&self) -> bool {
        match (self.atts.get("kind"), self.atts.get("sha1")) {
            (Some(k), None) if k == "file" => true,
            _ => false,
        }
    }
}

impl Estimate {
    fn update(&mut self, node: &SureTree) {
        for f in &node.files {
            if f.needs_hash() {
                self.files += 1;
                self.bytes += f.atts["size"].parse::<u64>().unwrap();
            }
            /*
            match (f.atts.get("kind"), f.atts.get("sha1")) {
                (Some(k), None) if k == "file" => {
                    self.files += 1;
                    self.bytes += f.atts["size"].parse::<u64>().unwrap();
                },
                _ => (),
            }
            */
        }

        for d in &node.children {
            self.update(d);
        }
    }
}

// TODO: Reuse buffer and hasher for a given thread.
fn hash_file<R: Read>(rd: &mut R) -> Result<Vec<u8>> {
    let mut h = Hasher::new(MessageDigest::sha1())?;
    let mut buf = vec![0u8; 8192];

    loop {
        let count = rd.read(&mut buf)?;
        if count == 0 {
            break;
        }

        h.write_all(&buf[0..count])?;
    }
    Ok(h.finish()?)
}

use self::atime_impl::noatime_open;

/// Open the given file, trying to not update the atime if that is
/// possible.
/// The `custom_flags` method is only stable since Rust 1.10.0.
#[cfg(target_os = "linux")]
mod atime_impl {
    use std::fs::{File, OpenOptions};
    use std::os::unix::fs::OpenOptionsExt;
    use std::io;
    use std::path::Path;

    // From linux's fcntl.h, not exported in the libc crate.
    const O_NOATIME: i32 = 0o1000000;

    pub fn noatime_open(name: &Path) -> io::Result<File> {
        // Try opening it first with noatime, and if that fails, try the open
        // again without the option.
        match OpenOptions::new().read(true).custom_flags(O_NOATIME).open(
            name,
        ) {
            Ok(f) => Ok(f),
            Err(_) => OpenOptions::new().read(true).open(name),
        }
    }
}

// Other platforms, just use normal open.
#[cfg(not(target_os = "linux"))]
mod atime_impl {
    use std::fs::{File, OpenOptions};
    use std::path::Path;
    use std::io;

    pub fn noatime_open(name: &Path) -> io::Result<File> {
        OpenOptions::new().read(true).open(name)
    }
}
