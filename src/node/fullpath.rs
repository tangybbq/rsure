//! Augment an iterator over nodes with something that tracks the full
//! path of the files involved.
//!
//! Unfortunately, Rust's Iter does not tie any lifetimes between the
//! iterator and the result of iteration (which is usually good).  This
//! makes it difficult to avoid computing these paths, however.
//!
//! If this becomes a performance bottleneck, we can come up with something
//! more complicated that avoids computing (and allocating) the result
//! paths for each node encountered.

use crate::{escape::Unescape, node::SureNode, Result};
use std::{
    ffi::OsString,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
};

pub fn into_tracker<I>(iter: I, root: &str) -> impl Iterator<Item = Result<PathedNode>>
where
    I: Iterator<Item = Result<SureNode>>,
{
    let root: OsString = OsStringExt::from_vec(root.unescape().unwrap());
    let mut cur = Path::new(&root).to_path_buf();
    let mut at_root = true;
    iter.map(move |node| {
        let node = node?;
        let path = match &node {
            SureNode::Enter { name, .. } => {
                // Don't add the pseudo "__root__" directory.
                if at_root {
                    if name != "__root__" {
                        panic!("Root directory not at root");
                    }
                    at_root = false;
                } else {
                    let name: OsString = OsStringExt::from_vec(name.unescape().unwrap());
                    cur.push(&name);
                }
                Some(cur.clone())
            }
            SureNode::File { name, .. } => {
                let name: OsString = OsStringExt::from_vec(name.unescape().unwrap());
                cur.push(&name);
                Some(cur.clone())
            }
            _ => None,
        };

        let do_pop = node.is_file() || node.is_leave();

        let result = Ok(PathedNode { node, path });

        if do_pop {
            cur.pop();
        }

        result
    })
}

#[derive(Debug)]
pub struct PathedNode {
    pub node: SureNode,
    pub path: Option<PathBuf>,
}

/*
pub trait PathTrack: Sized {
    fn into_tracker(self, root: &str) -> PathTracker<Self>;
}

impl<I: Iterator<Item = Result<SureNode>>> PathTrack for I {
    fn into_tracker(self, root: &str) -> PathTracker<I> {
        PathTracker {
            iter: self,
            root: Some(root.to_owned()),
            dirs: vec![],
        }
    }
}

pub struct PathTracker<I> {
    iter: I,
    root: Option<String>,
    dirs: Vec<String>,
}

#[derive(Debug)]
pub struct PathedNode {
    pub node: SureNode,
    pub path: Option<String>,
}

impl<I> Iterator for PathTracker<I>
    where I: Iterator<Item = Result<SureNode>>,
{
    type Item = Result<PathedNode>;

    fn next(&mut self) -> Option<Result<PathedNode>> {
        match self.iter.next() {
            None => None,
            Some(Err(e)) => Some(Err(e)),
            Some(Ok(node)) => {
                let path = match &node {
                    SureNode::Enter { name, .. } => {
                        // Don't add the pseudo "__root__ flag.
                        if self.dirs.is_empty() && name == "__root__" {
                            let root = self.root.take().unwrap();
                            self.dirs.push(root);
                        } else {
                            self.dirs.push(name.clone());
                        }
                        Some(self.dirs.join("/"))
                    }
                    SureNode::File { name, .. } => {
                        self.dirs.push(name.clone());
                        Some(self.dirs.join("/"))
                    }
                    _ => None,
                };

                let do_pop = node.is_file() || node.is_leave();

                let result = Some(Ok(PathedNode {
                    node: node,
                    path: path,
                }));

                if do_pop {
                    self.dirs.pop();
                }

                result
            }
        }
    }
}
*/
