//! Compare two iterator-based trees.

// This clippy seems to be broken, as it has some false triggers in this code.
#![allow(clippy::if_same_then_else)]

use crate::{node::SureNode, Error, Result};
use log::error;
use std::{collections::HashSet, path::Path};

/// This is the mutable state that is threaded through the recursive
/// traversal of the two trees.
struct State<IA, IB> {
    left: SureNode,
    right: SureNode,
    left_iter: IA,
    right_iter: IB,

    // Track warning messages about added and deleted attributes.
    adds: HashSet<String>,
    missings: HashSet<String>,

    // Attributes to be ignored
    ignore: HashSet<String>,
}

pub fn compare_trees<P: AsRef<Path>, IA, IB>(
    mut left: IA,
    mut right: IB,
    dir: P,
    ignore: &[&str],
) -> Result<()>
where
    IA: Iterator<Item = Result<SureNode>>,
    IB: Iterator<Item = Result<SureNode>>,
{
    let mut ignore: HashSet<String> = ignore.iter().map(|x| (*x).to_owned()).collect();
    // The ctime and ino will be different if a backup is restored, and we'd still like to get
    // meaningful results.  Add these to the list of ignored attributes.
    ignore.insert("ctime".to_owned());
    ignore.insert("ino".to_owned());

    let ln = match left.next() {
        None => return Err(Error::EmptyLeftIterator),
        Some(Err(e)) => return Err(e),
        Some(Ok(node)) => node,
    };
    let rn = match right.next() {
        None => return Err(Error::EmptyRightIterator),
        Some(Err(e)) => return Err(e),
        Some(Ok(node)) => node,
    };
    let mut state = State {
        left: ln,
        right: rn,
        left_iter: left,
        right_iter: right,
        adds: HashSet::new(),
        missings: HashSet::new(),
        ignore,
    };

    state.walk_root(dir.as_ref())
}

impl<IA, IB> State<IA, IB>
where
    IA: Iterator<Item = Result<SureNode>>,
    IB: Iterator<Item = Result<SureNode>>,
{
    /// Advance the left iterator.  If it sees the end, it will drop in a
    /// "Leave" node, which shouldn't be visited as long as the tree is
    /// well-formed.
    fn next_left(&mut self) -> Result<()> {
        let next = match self.left_iter.next() {
            None => SureNode::Leave,
            Some(Ok(node)) => node,
            Some(Err(e)) => return Err(e),
        };

        self.left = next;
        Ok(())
    }

    /// Advance the right iterator.  If it sees the end, it will drop in a
    /// "Leave" node, which shouldn't be visited as long as the tree is
    /// well-formed.
    fn next_right(&mut self) -> Result<()> {
        let next = match self.right_iter.next() {
            None => SureNode::Leave,
            Some(Ok(node)) => node,
            Some(Err(e)) => return Err(e),
        };

        self.right = next;
        Ok(())
    }

    fn walk_root(&mut self, dir: &Path) -> Result<()> {
        if !self.left.is_enter() {
            Err(Error::UnexpectedLeftNode)
        } else if !self.right.is_enter() {
            Err(Error::UnexpectedRightNode)
        } else if self.left.name() != "__root__" {
            Err(Error::IncorrectName)
        } else if self.right.name() != "__root__" {
            Err(Error::IncorrectName)
        } else {
            self.compare_enter(dir)?;
            self.next_left()?;
            self.next_right()?;
            self.walk_samedir(dir)
        }
    }

    /// We are within a directory (of the given name) where both trees have
    /// the same directory.  This will recursively compare any children,
    /// and once both have reached the separator, move to `walk_samefiles`.
    fn walk_samedir(&mut self, dir: &Path) -> Result<()> {
        loop {
            match (self.left.is_sep(), self.right.is_sep()) {
                (true, true) => {
                    self.next_left()?;
                    self.next_right()?;
                    return self.walk_samefiles(dir);
                }
                (false, true) => {
                    // The old trees has subdirectories not in this
                    // directory.
                    self.show_delete(dir);
                    self.next_left()?;
                    self.walk_leftdir()?;
                }
                (true, false) => {
                    // The new tree has a newly added directory.
                    self.show_add(dir);
                    self.next_right()?;
                    self.walk_rightdir()?;
                }
                _ if self.left.name() < self.right.name() => {
                    // Old subdirectory.
                    self.show_delete(dir);
                    self.next_left()?;
                    self.walk_leftdir()?;
                }
                _ if self.left.name() > self.right.name() => {
                    // The new tree has a newly added directory.
                    self.show_add(dir);
                    self.next_right()?;
                    self.walk_rightdir()?;
                }
                _ => {
                    // Same named directory.
                    let dirname = dir.join(self.left.name());
                    self.compare_enter(&dirname)?;
                    self.next_left()?;
                    self.next_right()?;
                    self.walk_samedir(&dirname)?;
                }
            }
        }
    }

    /// We are within the files section of the same directory in the two
    /// trees.  Walk through the nodes, reading the Leave node in both, and
    /// returning.
    fn walk_samefiles(&mut self, dir: &Path) -> Result<()> {
        loop {
            match (self.left.is_leave(), self.right.is_leave()) {
                (true, true) => {
                    self.next_left()?;
                    self.next_right()?;
                    return Ok(());
                }
                (false, true) => {
                    self.show_delete(dir);
                    self.next_left()?;
                }
                (true, false) => {
                    self.show_add(dir);
                    self.next_right()?;
                }
                _ if self.left.name() < self.right.name() => {
                    self.show_delete(dir);
                    self.next_left()?;
                }
                _ if self.left.name() > self.right.name() => {
                    self.show_add(dir);
                    self.next_right()?;
                }
                _ => {
                    // Same file.
                    let nodename = dir.join(self.left.name());
                    self.compare_file(&nodename)?;
                    self.next_left()?;
                    self.next_right()?;
                }
            }
        }
    }

    /// Old directory on the left tree.  Walk through nodes recursively to
    /// discard entire tree.
    fn walk_leftdir(&mut self) -> Result<()> {
        loop {
            if self.left.is_enter() {
                self.next_left()?;
                self.walk_leftdir()?;
            } else if self.left.is_leave() {
                self.next_left()?;
                return Ok(());
            } else {
                self.next_left()?;
            }
        }
    }

    /// New directory on the right tree.  Walk through nodes recursively to
    /// discard entire tree.
    fn walk_rightdir(&mut self) -> Result<()> {
        loop {
            if self.right.is_enter() {
                self.next_right()?;
                self.walk_rightdir()?;
            } else if self.right.is_leave() {
                self.next_right()?;
                return Ok(());
            } else {
                self.next_right()?;
            }
        }
    }

    /// Print a message about something added (the name will be the thing
    /// on the right.
    fn show_add(&self, dir: &Path) {
        println!(
            "+ {:22} {:?}",
            self.right.kind(),
            dir.join(self.right.name())
        );
    }

    /// Print a message about something removed (the name will be the thing
    /// on the left.
    fn show_delete(&self, dir: &Path) {
        println!("- {:22} {:?}", self.left.kind(), dir.join(self.left.name()));
    }

    /// Compare the two "Enter" nodes we are visiting.
    fn compare_enter(&mut self, dir: &Path) -> Result<()> {
        self.compare_atts('d', dir)
    }

    /// Compare two file nodes.
    fn compare_file(&mut self, dir: &Path) -> Result<()> {
        self.compare_atts('f', dir)
    }

    /// Attribute comparison.
    fn compare_atts(&mut self, _kind: char, dir: &Path) -> Result<()> {
        let mut old = self.left.atts().unwrap().clone();
        let mut new = self.right.atts().unwrap().clone();
        let mut diffs = vec![];

        for att in self.ignore.iter() {
            old.remove(att);
            new.remove(att);
        }

        for (k, v) in &new {
            match old.get(k) {
                None => {
                    // This attribute is in the new tree, but not the old
                    // one, warn, but only once.
                    if !self.adds.contains(k) {
                        error!("Added attribute: {}", k);
                        self.adds.insert(k.clone());
                    }
                }
                Some(ov) => {
                    if v != ov {
                        diffs.push(k.clone());
                    }
                }
            }
            old.remove(k);
        }

        for k in old.keys() {
            if !self.missings.contains(k) {
                error!("Missing attribute: {}", k);
                self.missings.insert(k.clone());
            }
        }

        if diffs.len() > 0 {
            let mut buf = String::new();
            diffs.sort();
            for d in &diffs {
                if !buf.is_empty() {
                    buf.extend(",".chars());
                }
                buf.extend(d.chars());
            }
            println!("  [{:<20}] {:?}", buf, dir);
        }

        Ok(())
    }
}
