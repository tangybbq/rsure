// Test the naming convention code.

extern crate tempdir;
extern crate weave;

use std::path::Path;

use tempdir::TempDir;
use weave::{NamingConvention, SimpleNaming};

#[test]
fn test_names() {
    let tmp = TempDir::new("weave").unwrap();

    let path = tmp.path().to_str().unwrap();

    let nm = SimpleNaming::new(tmp.path(), "sample", "weave", true);
    assert_eq!(
        nm.main_file(),
        Path::new(&format!("{}/sample.weave.gz", path))
    );
    assert_eq!(
        nm.backup_file(),
        Path::new(&format!("{}/sample.bak.gz", path))
    );

    for i in 0..100 {
        let (tname, _tfd) = nm.temp_file().unwrap();
        assert_eq!(tname, Path::new(&format!("{}/sample.{}.gz", path, i)));
        println!("tname: {:?}", tname);
    }
}
