// Comparisons between trees.

use crate::compvisit::{CompareAction, CompareType, CompareVisitor};
use log::{error, log};
use std::collections::BTreeMap;
use std::path::Path;

use super::suretree::{AttMap, PathAdd, SureTree};

pub trait TreeUpdate {
    /// Update any sha1 hashes in `self` using `old` as a reference.
    /// This looks for files that have sufficiently similar attributes that
    /// we can assume the sha1 hash is the same.
    fn update_from(&mut self, old: &Self);
}

impl TreeUpdate for SureTree {
    fn update_from(&mut self, old: &SureTree) {
        walk(self, old);
    }
}

fn walk(new: &mut SureTree, old: &SureTree) {
    // Walk all of the directories that are possible.
    let old_children: BTreeMap<&str, &SureTree> =
        old.children.iter().map(|ch| (&ch.name[..], ch)).collect();

    for ch in &mut new.children {
        old_children.get(&ch.name[..]).map(|och| walk(ch, och));
    }

    // Walk the file nodes that are the same, and see if they can be
    // updated.
    let old_files: BTreeMap<&str, &AttMap> = old
        .files
        .iter()
        .map(|ch| (&ch.name[..], &ch.atts))
        .collect();

    for file in &mut new.files {
        let atts = match old_files.get(&file.name[..]) {
            None => continue,
            Some(atts) => atts,
        };

        // If new already has a node, don't do anything.  This shouldn't
        // normally happen.
        if file.atts.contains_key("sha1") {
            continue;
        }

        // Only compare files.
        if file.atts["kind"] != "file" || atts["kind"] != "file" {
            continue;
        }

        if file.atts.get("ino") != atts.get("ino") || file.atts.get("ctime") != atts.get("ctime") {
            continue;
        }

        // Make sure there is actually a sha1 to get.
        match atts.get("sha1") {
            None => continue,
            Some(v) => {
                file.atts.insert("sha1".to_string(), v.to_string());
            }
        }
    }
}

pub trait TreeCompare {
    /// Compare two trees, reporting (to stdout) any differences between
    /// them.
    fn compare_from<V: CompareVisitor>(&self, visitor: &mut V, old: &Self, path: &Path);
}

impl TreeCompare for SureTree {
    fn compare_from<V: CompareVisitor>(&self, visitor: &mut V, old: &Self, path: &Path) {
        compwalk(self, old, visitor, path);
    }
}

fn compwalk<V: CompareVisitor>(new: &SureTree, old: &SureTree, visitor: &mut V, path: &Path) {
    // Walk and compare directories.
    let mut old_children: BTreeMap<&String, &SureTree> =
        old.children.iter().map(|ch| (&ch.name, ch)).collect();

    for ch in &new.children {
        let cpath = ch.join(&path);
        match old_children.get(&ch.name) {
            None => visitor.visit(&cpath, CompareType::Dir, CompareAction::Add, None),
            Some(och) => compwalk(ch, och, visitor, &cpath),
        }
        old_children.remove(&ch.name);
    }

    // Print out any directories that have been removed.
    // TODO: This print out of order.
    for &name in old_children.keys() {
        visitor.visit(
            &name.join(&path),
            CompareType::Dir,
            CompareAction::Delete,
            None,
        );
    }

    // Walk and compare files.
    let mut old_files: BTreeMap<&str, &AttMap> = old
        .files
        .iter()
        .map(|ch| (&ch.name[..], &ch.atts))
        .collect();

    for file in &new.files {
        let fpath = file.join(&path);
        match old_files.get(&file.name[..]) {
            None => visitor.visit(&fpath, CompareType::NonDir, CompareAction::Add, None),
            Some(atts) => attr_comp(atts, &file.atts, visitor, &fpath),
        }
        old_files.remove(&file.name[..]);
    }

    // Print out any files that have been removed.
    for name in old_files.keys() {
        visitor.visit(
            &name.join(&path),
            CompareType::NonDir,
            CompareAction::Delete,
            None,
        );
    }
}

// Compare the old and new attributes, formatting a message if they differ.
fn attr_comp<V: CompareVisitor>(old: &AttMap, new: &AttMap, visitor: &mut V, name: &Path) {
    let mut new = new.clone();
    let mut old = old.clone();
    let mut diffs = vec![];

    // The ctime and ino will be different if a backup is restored, and
    // we'd still like to get meaningful results out of it.
    old.remove("ctime");
    new.remove("ctime");
    old.remove("ino");
    new.remove("ino");

    for (k, v) in &new {
        match old.get(k) {
            None => error!("Added attribute: {}", k),
            Some(ov) => {
                if v != ov {
                    diffs.push(k.clone());
                }
            }
        }
        old.remove(k);
    }

    for k in old.keys() {
        error!("Missing attribute: {}", k);
    }

    if diffs.len() > 0 {
        visitor.visit(
            name,
            CompareType::NonDir,
            CompareAction::Modify,
            Some(&diffs),
        );
    }
}
