// Bitkeeper command line utilities.

use regex::Regex;
use Result;
use rsure::bk::{self, BkDir};
use rsure::SureTree;
use std::collections::HashSet;
use std::os::linux::fs::MetadataExt;
use std::path::Path;

pub fn new(path: &str) -> Result<()> {
    try!(ensure_dir(path));
    try!(bk::setup(path));
    Ok(())
}

/// The given path must name either a not-yet-existent directory, or be an
/// empty directory.  Ensure that is the case.
///
pub fn ensure_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();

    if path.is_dir() {
        for ent in try!(path.read_dir()) {
            let ent = try!(ent);
            return Err(format!("Directory {:?} is not empty (contains {:?}",
                               path, ent.path()).into());
        }
        Ok(())
    } else if path.exists() {
        Err(format!("Path {:?} names something other than a directory", path).into())
    } else {
        Ok(())
    }
}

/// Import a bunch of files from `src` and include them in the bkdir.
pub fn import<P1: AsRef<Path>, P2: AsRef<Path>>(src: P1, dest: P2) -> Result<()> {
    let src = src.as_ref();
    let bkd = try!(BkDir::new(dest));

    let re = Regex::new(r"^([^-]+)-(.*)\.dat\.gz$").unwrap();

    let mut namedir = vec![];

    let present = try!(bkd.query());
    let present = present.iter().map(|p| (&p.name, &p.file)).collect::<HashSet<_>>();

    println!("{} surefiles already present", present.len());
    for ent in try!(src.read_dir()) {
        let ent = try!(ent);
        let name = ent.file_name();
        let name = match name.to_str() {
            None => continue,
            Some(name) => name,
        };
        match re.captures(name) {
            None => continue,
            Some(cap) => {
                // println!("ent: {:?} name {:?}",
                //          cap.at(1).unwrap(),
                //          cap.at(2).unwrap());
                let mtime = try!(ent.metadata()).st_mtime();
                namedir.push(ImportNode {
                    mtime: mtime,
                    name: cap.at(2).unwrap().to_owned(),
                    file: cap.at(1).unwrap().to_owned(),
                });
            },
        }
    }
    namedir.sort();
    for node in namedir {
        let file = format!("{}.dat", node.file);
        if present.contains(&(&node.name, &file)) {
            continue;
        }
        let name = format!("{}-{}.dat.gz", node.file, node.name);
        println!("Importing: {:?} ({:?}, {:?})", name, node.name, file);
        let tree = try!(SureTree::load(&src.join(name)));
        try!(bkd.save(&tree, &file, &node.name));
    }
    Ok(())
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
struct ImportNode {
    mtime: i64,
    name: String,
    file: String,
}
