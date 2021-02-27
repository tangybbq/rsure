/// Sure tree scanning from the filesystem.
use crate::{
    escape::Escape, node::SureNode, progress::ScanProgress, surefs::encode_atts, suretree::AttMap,
    Error, Result,
};
use log::error;
use std::{
    collections::VecDeque,
    fs::{self, symlink_metadata, Metadata},
    os::unix::prelude::*,
    path::{Path, PathBuf},
};

pub fn walk<P: AsRef<Path>>(root: P) -> Result<()> {
    for entry in scan_fs(root)? {
        let entry = entry?;
        println!("{:?}", entry);
    }

    Ok(())
}

/// A filesystem scanner walks a filesystem, iterating over a tree as it is
/// encountered.
pub fn scan_fs<P: AsRef<Path>>(root: P) -> Result<ScanIterator> {
    let root = root.as_ref().to_path_buf();
    let meta = symlink_metadata(&root)?;

    if !meta.is_dir() {
        return Err(Error::RootMustBeDir);
    }

    let atts = encode_atts(&root, &meta);
    let root_dev = meta.dev();
    let mut todo = VecDeque::new();
    todo.push_back(AugNode::SubDir {
        path: root,
        name: "__root__".to_string(),
        meta: meta,
        atts: atts,
    });

    let si = ScanIterator {
        todo: todo,
        root_dev: root_dev,
        progress: ScanProgress::new(),
    };

    Ok(si)
}

pub struct ScanIterator {
    todo: VecDeque<AugNode>,
    root_dev: u64,
    progress: ScanProgress,
}

impl Iterator for ScanIterator {
    type Item = Result<SureNode>;

    fn next(&mut self) -> Option<Result<SureNode>> {
        match self.todo.pop_front() {
            None => None,
            Some(AugNode::Normal(e)) => Some(Ok(e)),
            Some(AugNode::SubDir {
                path,
                name,
                atts,
                meta,
            }) => {
                // Push the contents of this directory.  Unless we have
                // crossed a mountpoint.
                if !meta.is_dir() || meta.dev() == self.root_dev {
                    match self.push_dir(&path) {
                        Ok(()) => (),
                        Err(e) => return Some(Err(e)),
                    };
                } else {
                    self.push_empty_dir();
                }

                Some(Ok(SureNode::Enter {
                    name: name,
                    atts: atts,
                }))
            }
        }
    }
}

impl ScanIterator {
    fn push_dir(&mut self, path: &Path) -> Result<()> {
        let mut entries = vec![];

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            entries.push(entry);
        }

        // Sort by inode first.  This helps performance on some filesystems
        // (such as ext4).
        entries.sort_by(|a, b| a.ino().cmp(&b.ino()));

        let mut files: Vec<_> = entries
            .iter()
            .filter_map(|e| match e.metadata() {
                Ok(m) => {
                    let path = e.path();
                    let atts = encode_atts(&path, &m);

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
            })
            .collect();

        // Sort them back by name.
        files.sort_by(|a, b| a.path.file_name().cmp(&b.path.file_name()));

        let (dirs, files): (Vec<_>, Vec<_>) = files.into_iter().partition(|n| n.meta.is_dir());

        self.progress.update(
            dirs.len() as u64,
            files.len() as u64,
            files.iter().map(|x| x.meta.len()).sum(),
        );

        self.todo.push_front(AugNode::Normal(SureNode::Leave));

        // The files in reverse order.
        for f in files.into_iter().rev() {
            self.todo.push_front(AugNode::Normal(SureNode::File {
                name: f.path.file_name().unwrap().as_bytes().escaped(),
                atts: f.atts,
            }));
        }

        self.todo.push_front(AugNode::Normal(SureNode::Sep));

        // The dirs in reverse order.
        for d in dirs.into_iter().rev() {
            let name = d.path.file_name().unwrap().as_bytes().escaped();
            self.todo.push_front(AugNode::SubDir {
                path: d.path,
                name: name,
                meta: d.meta,
                atts: d.atts,
            });
        }

        Ok(())
    }

    /// Pushes the Sep and Leave needed to make an empty directory work.
    /// Used when skipping directories that cross mountpoints.
    fn push_empty_dir(&mut self) {
        self.todo.push_front(AugNode::Normal(SureNode::Leave));
        self.todo.push_front(AugNode::Normal(SureNode::Sep));
    }
}

struct OneFile {
    path: PathBuf,
    meta: Metadata,
    atts: AttMap,
}

/// Augmented entries.  This intersperses regular nodes with special ones
/// containing enough information to add subdirectories.
enum AugNode {
    Normal(SureNode),
    SubDir {
        path: PathBuf,
        name: String,
        meta: Metadata,
        atts: AttMap,
    },
}
