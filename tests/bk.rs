// Test that the BK storage works.

extern crate rsure;
extern crate tempdir;

use rsure::bk;
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
