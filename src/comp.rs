// Comparisons between trees.

use std::collections::BTreeMap;

use super::suretree::{AttMap, SureTree};

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
    let old_children: BTreeMap<&String, &SureTree> =
        old.children.iter().map(|ch| (&ch.name, ch)).collect();

    for ch in &mut new.children {
        old_children.get(&ch.name).map(|och| walk(ch, och));
    }

    // Walk the file nodes that are the same, and see if they can be
    // updated.
    let old_files: BTreeMap<&String, &AttMap> =
        old.files.iter().map(|ch| (&ch.name, &ch.atts)).collect();

    for file in &mut new.files {
        let atts = match old_files.get(&file.name) {
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
            file.atts.get("ctime") != atts.get("ctime") ||
            file.atts.get("size") != atts.get("size")
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
