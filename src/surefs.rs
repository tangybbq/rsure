// Filesystem scanning.

use crate::{escape::*, suretree::AttMap};
use log::error;

use std::{
    fs::{self, Metadata},
    os::unix::prelude::*,
    path::Path,
};

// Encode the attributes for the given node.  Note that this returns, even
// when there is an error (resolving a symlink).  It logs an error, and
// returns a placeholder.
pub(crate) fn encode_atts(name: &Path, meta: &Metadata) -> AttMap {
    // let fname = name.file_name().unwrap().as_bytes().escaped();
    let mode = meta.mode() as libc::mode_t & libc::S_IFMT;

    let mut base = AttMap::new();

    // These attributes apply to every node.
    base.insert("uid".to_string(), meta.uid().to_string());
    base.insert("gid".to_string(), meta.gid().to_string());
    base.insert(
        "perm".to_string(),
        (meta.mode() as libc::mode_t & !libc::S_IFMT).to_string(),
    );

    // Other permissions are based on the type of the node.
    match mode as libc::mode_t {
        libc::S_IFDIR => {
            base.insert("kind".to_string(), "dir".to_string());
        }
        libc::S_IFREG => {
            base.insert("kind".to_string(), "file".to_string());
            base.insert("ino".to_string(), meta.ino().to_string());
            base.insert("size".to_string(), meta.size().to_string());
            time_info(&mut base, meta);
            // Note that the 'sha1' attribute is computed later.
        }
        libc::S_IFLNK => {
            base.insert("kind".to_string(), "lnk".to_string());
            let link = match fs::read_link(name) {
                Ok(l) => l,
                Err(err) => {
                    error!("Unable to read link: {:?} ({})", name, err);
                    // TODO: Generate a unique placeholder so this will
                    // always show up.
                    From::from("???")
                }
            };
            base.insert("targ".to_string(), link.as_os_str().as_bytes().escaped());
        }
        libc::S_IFIFO => {
            base.insert("kind".to_string(), "fifo".to_string());
        }
        libc::S_IFSOCK => {
            base.insert("kind".to_string(), "sock".to_string());
        }
        libc::S_IFCHR => {
            base.insert("kind".to_string(), "chr".to_string());
            add_dev(&mut base, meta);
        }
        libc::S_IFBLK => {
            base.insert("kind".to_string(), "blk".to_string());
            add_dev(&mut base, meta);
        }
        _ => panic!("Unknown file type: 0o{:o}", mode),
    }

    // println!("{:?}: atts: {:?}", fname, base);
    base
}

fn add_dev(base: &mut AttMap, meta: &Metadata) {
    let rdev = meta.rdev();
    // This is defined in a macro, and hasn't made it into libc.  Given how
    // it is defined in the header, it is unlikely to change, at least on
    // Linux.
    base.insert("devmaj".to_string(), ((rdev >> 8) & 0xfff).to_string());
    base.insert("devmin".to_string(), (rdev & 0xff).to_string());
}

fn time_info(base: &mut AttMap, meta: &Metadata) {
    // TODO: Handle the nsec part of the time.
    base.insert("mtime".to_string(), meta.mtime().to_string());
    base.insert("ctime".to_string(), meta.ctime().to_string());
}
