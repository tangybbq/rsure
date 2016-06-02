// Bitkeeper command line utilities.

use Result;
use rsure::bk;
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
