// Test that the BK storage works.

extern crate rsure;
extern crate tempdir;

use rsure::bk::{self, BkDir};
use rsure::{PrintCompare, TreeCompare};
use std::path::Path;
use std::process::Command;
use tempdir::TempDir;

#[test]
fn bk_create() {
    let tmp = TempDir::new("rsure").unwrap();

    // Can we create in an empty directory.
    bk::setup(tmp.path()).unwrap();
    verify_configs(tmp.path());

    // Make sure we can also create a directory.
    let sub = tmp.path().join("subdir");
    bk::setup(&sub).unwrap();
    verify_configs(&sub);

    // It seems that BitKeeper will create as many directories as
    // necessary.
    let sub = tmp.path().join("first").join("second");
    bk::setup(&sub).unwrap();
}

#[test]
fn bk_saves() {
    let tmp = TempDir::new("rsure").unwrap();

    bk::setup(tmp.path()).unwrap();
    let bkdir = BkDir::new(tmp.path()).unwrap();

    let tree = rsure::scan_fs(tmp.path()).unwrap();
    bkdir.save(&tree, "self.dat", "first-version").unwrap();

    let t2 = rsure::scan_fs(tmp.path()).unwrap();
    bkdir.save(&t2, "self.dat", "second-version").unwrap();

    // tmp.into_path();
    println!("Running query!!!");
    let qy = bkdir.query().unwrap();
    let mut found = 0;
    for ent in &qy {
        if ent.file == "self.dat" {
            if ent.name == "first-version" {
                found |= 1;
            } else if ent.name == "second-version" {
                found |= 2;
            }
        }
    }
    assert_eq!(found, 3);

    match bkdir.load("none.dat", "first-version") {
        Err(_) => (),
        Ok(_) => panic!("Shouldn't be able to find node."),
    }

    match bkdir.load("self.dat", "third-version") {
        Err(_) => (),
        Ok(_) => panic!("Shouldn't be able to find node."),
    }

    let tt1 = bkdir.load("self.dat", "first-version").unwrap();
    let tt2 = bkdir.load("self.dat", "second-version").unwrap();
    let mut comp = PrintCompare;
    tt2.compare_from(&mut comp, &tt1, tmp.path());
}

fn verify_configs(base: &Path) {
    verify_config(base, "BAM", b"off\n");
    verify_config(base, "checkout", b"none\n");
}

fn verify_config(base: &Path, name: &str, expect: &[u8]) {
    let mut cmd = Command::new("bk");
    cmd.args(&["config", name]);
    cmd.current_dir(base);
    let out = cmd.output().unwrap();
    assert_eq!(out.stdout, expect);
    assert_eq!(out.stderr, b"");
    // println!("stdout: {:?}", String::from_utf8_lossy(&out.stdout));
    // println!("stderr: {:?}", String::from_utf8_lossy(&out.stderr));
}
