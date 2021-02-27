//! Computing hashes for files.

use crate::Result;
use openssl::hash::{DigestBytes, Hasher, MessageDigest};
use std::io::{Read, Write};
#[derive(Debug)]
pub struct Estimate {
    pub files: u64,
    pub bytes: u64,
}

// TODO: Reuse buffer and hasher for a given thread.
pub(crate) fn hash_file<R: Read>(rd: &mut R) -> Result<DigestBytes> {
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

pub(crate) use self::atime_impl::noatime_open;

/// Open the given file, trying to not update the atime if that is
/// possible.
/// The `custom_flags` method is only stable since Rust 1.10.0.
#[cfg(target_os = "linux")]
mod atime_impl {
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Path;

    // From linux's fcntl.h, not exported in the libc crate.
    const O_NOATIME: i32 = 0o1000000;

    pub fn noatime_open(name: &Path) -> io::Result<File> {
        // Try opening it first with noatime, and if that fails, try the open
        // again without the option.
        match OpenOptions::new()
            .read(true)
            .custom_flags(O_NOATIME)
            .open(name)
        {
            Ok(f) => Ok(f),
            Err(_) => OpenOptions::new().read(true).open(name),
        }
    }
}

// Other platforms, just use normal open.
#[cfg(not(target_os = "linux"))]
mod atime_impl {
    use std::fs::{File, OpenOptions};
    use std::io;
    use std::path::Path;

    pub fn noatime_open(name: &Path) -> io::Result<File> {
        OpenOptions::new().read(true).open(name)
    }
}
