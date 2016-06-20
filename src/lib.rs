// Rsure library.

extern crate flate2;
extern crate libc;
extern crate openssl;
extern crate regex;
extern crate rustc_serialize;
extern crate time;

#[macro_use]
extern crate log;

#[macro_use]
extern crate error_chain;

use std::path::Path;

pub use surefs::scan_fs;
pub use hashes::SureHash;
pub use suretree::SureTree;
pub use comp::{TreeCompare, TreeUpdate};
pub use show::show_tree;
pub use progress::Progress;

pub use errors::{Error, ErrorKind, ChainErr, Result};

mod errors;
mod escape;
mod show;
mod suretree;
mod surefs;
mod hashes;
mod comp;
mod progress;
pub mod bk;

// Some common operations, abstracted here.

/// Perform an update scan, using the information in 'src' to possibly
/// reduce the hash effort needed when scanning 'dir'.  If 'src' is None,
/// consider this a fresh scan, and hash all files.
pub fn update<P1, P2, P3>(dir: P1, src: Option<P2>, dest: P3) -> Result<()>
    where P1: AsRef<Path>,
          P2: AsRef<Path>,
          P3: AsRef<Path>
{
    let dir = dir.as_ref();
    let src = src.as_ref().map(|p| p.as_ref());
    let dest = dest.as_ref();

    let mut new_tree = try!(scan_fs(dir));

    match src {
        None => (),
        Some(src) => {
            let old_tree = try!(SureTree::load(src));
            new_tree.update_from(&old_tree);
        },
    }

    let estimate = new_tree.hash_estimate();
    let mut progress = Progress::new(estimate.files, estimate.bytes);
    new_tree.hash_update(dir, &mut progress);
    progress.flush();
    try!(new_tree.save(dest));
    Ok(())
}

/// Until feature default_type_parameter_fallback comes into stable, this
/// function will return a `None` appropriately typed for `update` above.
pub fn no_path() -> Option<&'static str> { None }
