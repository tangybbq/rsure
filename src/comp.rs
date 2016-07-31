// Comparisons between trees.

use std::collections::BTreeMap;
use std::io::prelude::*;
use std::path::Path;

use super::suretree::{AttMap, SureTree, PathAdd};

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
    let old_files: BTreeMap<&str, &AttMap> =
        old.files.iter().map(|ch| (&ch.name[..], &ch.atts)).collect();

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

        if file.atts.get("ino") != atts.get("ino") ||
            file.atts.get("ctime") != atts.get("ctime")
        {
            continue;
        }

        // Make sure there is actually a sha1 to get.
        match atts.get("sha1") {
            None => continue,
            Some(v) => {
                file.atts.insert("sha1".to_string(), v.to_string());
            },
        }
    }
}

/// A `CompareAction` receives information about the changes made in two
/// trees.
pub trait CompareAction {
    fn add_dir(&mut self, name: &Path);
    fn del_dir(&mut self, name: &Path);
    fn add_file(&mut self, name: &Path);
    fn del_file(&mut self, name: &Path);
    fn att_change(&mut self, name: &Path, atts: &[String]);
}

/// `PrintCompare` just prints out the differences.
pub struct PrintCompare;

impl CompareAction for PrintCompare {
    fn add_dir(&mut self, name: &Path) {
        println!("+ {:22} {}", "dir", name.to_string_lossy());
    }

    fn del_dir(&mut self, name: &Path) {
        println!("- {:22} {}", "dir", name.to_string_lossy());
    }

    fn add_file(&mut self, name: &Path) {
        println!("+ {:22} {}", "file", name.to_string_lossy());
    }

    fn del_file(&mut self, name: &Path) {
        println!("- {:22} {}", "file", name.to_string_lossy());
    }

    fn att_change(&mut self, name: &Path, atts: &[String]) {
        let mut message = vec![];
        for ent in atts {
            write!(&mut message, ",{}", ent).unwrap();
        }
        let message = String::from_utf8(message).unwrap();
        println!("  [{:<20}] {}", &message[1..], name.to_string_lossy());
    }
}

pub trait TreeCompare {
    /// Compare two trees, reporting (to stdout) any differences between
    /// them.
    fn compare_from<C: CompareAction>(&self, action: &mut C, old: &Self, path: &Path);
}

impl TreeCompare for SureTree {
    fn compare_from<C: CompareAction>(&self, action: &mut C, old: &Self, path: &Path) {
        compwalk(self, old, action, path);
    }
}

fn compwalk<C: CompareAction>(new: &SureTree, old: &SureTree, action: &mut C, path: &Path) {
    // Walk and compare directories.
    let mut old_children: BTreeMap<&String, &SureTree> =
        old.children.iter().map(|ch| (&ch.name, ch)).collect();

    for ch in &new.children {
        let cpath = ch.join(&path);
        match old_children.get(&ch.name) {
            None => action.add_dir(&cpath),
            Some(och) => compwalk(ch, och, action, &cpath),
        }
        old_children.remove(&ch.name);
    }

    // Print out any directories that have been removed.
    // TODO: This print out of order.
    for &name in old_children.keys() {
        action.del_dir(&name.join(&path));
    }

    // Walk and compare files.
    let mut old_files: BTreeMap<&str, &AttMap> =
        old.files.iter().map(|ch| (&ch.name[..], &ch.atts)).collect();

    for file in &new.files {
        let fpath = file.join(&path);
        match old_files.get(&file.name[..]) {
            None => action.add_file(&fpath),
            Some(atts) => attr_comp(atts, &file.atts, action, &fpath),
        }
        old_files.remove(&file.name[..]);
    }

    // Print out any files that have been removed.
    for name in old_files.keys() {
        action.del_file(&name.join(&path));
    }
}

// Compare the old and new attributes, formatting a message if they differ.
fn attr_comp<C: CompareAction>(old: &AttMap, new: &AttMap, action: &mut C, name: &Path) {
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
            Some(ov) => if v != ov {
                diffs.push(k.clone());
            },
        }
        old.remove(k);
    }

    for k in old.keys() {
        error!("Missing attribute: {}", k);
    }

    if diffs.len() > 0 {
        action.att_change(name, &diffs);
    }
}
