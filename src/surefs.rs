// Filesystem scanning.

use Result;
use escape::*;
use suretree::{AttMap, SureFile, SureTree};

use std::fs::{self, symlink_metadata, Metadata};
use std::os::unix::prelude::*;
use std::path::{Path, PathBuf};
use libc;

pub fn scan_fs<P: AsRef<Path>>(root: P) -> Result<SureTree> {
    let root = root.as_ref().to_path_buf();

    walk_root(&root)
}

fn walk_root(path: &Path) -> Result<SureTree> {
    let meta = symlink_metadata(path)?;

    if !meta.is_dir() {
        return Err(From::from("Root must be a directory"));
    }

    walk(
        "__root__".to_string(),
        path,
        encode_atts(path, &meta),
        &meta,
    )
}

fn walk(my_name: String, path: &Path, my_atts: AttMap, my_meta: &Metadata) -> Result<SureTree> {
    let mut entries = vec![];

    // TODO: Instead of failing everything because of read failure, just
    // fail some things.

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        entries.push(entry);
    }

    // Sort by inode before sorting.  This helps performance on some
    // filesystems (such as ext4).
    entries.sort_by(|a, b| a.ino().cmp(&b.ino()));

    let mut files: Vec<_> = entries
        .iter()
        .filter_map(|e| {
            match e.metadata() {
                Ok(m) => {
                    let path = e.path();
                    let atts = encode_atts(&path, &m);

                    // Check for crossing mountpoints.
                    if m.is_dir() && m.dev() != my_meta.dev() {
                        return None;
                    }

                    Some(OneFile {
                        path: path,
                        meta: m,
                        atts: atts,
                    })
                }
                Err(err) => {
                    error!("Unable to stat file: {:?} ({})", e.path(), err);
                    None
                }
            }
        })
        .collect();

    // Sort them back by name.
    files.sort_by(|a, b| a.path.file_name().cmp(&b.path.file_name()));

    // Build the SureTree data.
    let mut node = SureTree {
        name: my_name,
        atts: my_atts,
        children: Vec::new(),
        files: Vec::new(),
    };

    // Process all of the nodes.
    for ch in files {
        if ch.meta.is_dir() {
            let child_name = ch.path.file_name().unwrap().as_bytes().escaped();
            let child = walk(child_name, &ch.path, ch.atts, &ch.meta)?;
            node.children.push(child);
        } else {
            let child_name = ch.path.file_name().unwrap().as_bytes().escaped();
            let child = SureFile {
                name: child_name,
                atts: ch.atts,
            };
            node.files.push(child);
        }
    }

    Ok(node)
}

// Encode the attributes for the given node.  Note that this returns, even
// when there is an error (resolving a symlink).  It logs an error, and
// returns a placeholder.
fn encode_atts(name: &Path, meta: &Metadata) -> AttMap {
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

// Temp struct to hold information on intermediate files.
struct OneFile {
    path: PathBuf,
    meta: Metadata,
    atts: AttMap,
}
