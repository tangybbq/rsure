//! Use bitkeeper to manage multiple versions of surefiles.
//!
//! Now that BitKeeper is [open source](http://bitkeeper.org/), let's make
//! it available as a store for surefiles.  Some brief experimenting shows
//! that the 'weave' method BitKeeper uses to store multiple file revisions
//! works rather well for surefiles.  One test case, for example, was able
//! to take nearly 2GB of individually compressed surefiles (several
//! hundred), and encode them in less than 50MB.

use Result;
use std::path::Path;
use std::process::Command;

/// Initialize a new BitKeeper-based storage directory.  The path should
/// name a directory that is either empty or can be created with a single
/// `mkdir`.
pub fn setup<P: AsRef<Path>>(base: P) -> Result<()> {
    let mut cmd = Command::new("bk");
    // BAM=off is needed to keep BK from storing large files as just whole
    // files.  Surefiles will often be large, and the delta storage is the
    // whole reason we're using BK.
    // checkout=none frees up some space by not leaving uncompressed copies
    // of the surefiles in the work directory.
    cmd.args(&["setup", "-f", "-FBAM=off", "-Fcheckout=none"]);
    cmd.arg(base.as_ref().as_os_str());
    let status = try!(cmd.status());
    if !status.success() {
        return Err(format!("Error running bk: {:?}", status).into());
    }
    Ok(())
}
