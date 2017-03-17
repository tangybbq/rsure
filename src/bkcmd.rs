// Bitkeeper command line utilities.

use regex::Regex;
use Result;
use rsure::bk_setup;
use rsure::{BkStore, Store, SureTree};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use std::os::unix::fs::MetadataExt;

pub fn new(path: &str) -> Result<()> {
    ensure_dir(path)?;
    bk_setup(path)?;
    Ok(())
}

/// The given path must name either a not-yet-existent directory, or be an
/// empty directory.  Ensure that is the case.
///
pub fn ensure_dir<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();

    if path.is_dir() {
        for ent in path.read_dir()? {
            let ent = ent?;
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
    let mut bkd = BkStore::new(dest.as_ref(), "");

    let re = Regex::new(r"^([^-]+)-(.*)\.dat\.gz$").unwrap();

    let mut namedir = vec![];

    let present = bkd.query()?;
    let present = present.iter().map(|p| (&p.name, &p.file)).collect::<HashSet<_>>();

    println!("{} surefiles already present", present.len());
    for ent in src.read_dir()? {
        let ent = ent?;
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
                let mtime = ent.metadata()?.mtime();
                namedir.push(ImportNode {
                    mtime: mtime,
                    name: cap.get(2).unwrap().as_str().to_owned(),
                    file: cap.get(1).unwrap().as_str().to_owned(),
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
        let tree = SureTree::load(&src.join(name))?;
        bkd.name = file.to_string();
        let mut tags = BTreeMap::new();
        tags.insert("name".to_string(), node.name.clone());
        bkd.write_new(&tree, &tags)?;
    }
    Ok(())
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
struct ImportNode {
    mtime: i64,
    name: String,
    file: String,
}
