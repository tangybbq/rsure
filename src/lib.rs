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

use std::collections::BTreeMap;
use std::path::Path;

pub use surefs::scan_fs;
pub use hashes::SureHash;
pub use suretree::SureTree;
pub use comp::{TreeCompare, TreeUpdate};
pub use compvisit::{CompareVisitor, CompareType, CompareAction, PrintVisitor,
                    stdout_visitor, stderr_visitor};
pub use show::show_tree;
pub use progress::Progress;

pub use errors::{Error, ErrorKind, ChainErr, Result};

pub use store::{StoreTags, Store, Version, parse_store};

mod errors;
mod escape;
mod show;
mod suretree;
mod surefs;
mod hashes;
mod comp;
mod compvisit;
mod progress;
mod store;
pub mod bk;

// Some common operations, abstracted here.

/// Perform an update scan, using the given store.  If 'update' is true, use the hashes from a
/// previous run, otherwise perform a fresh scan.
pub fn update<P: AsRef<Path>>(dir: P, store: &Store, is_update: bool) -> Result<()> {
    let dir = dir.as_ref();

    let mut new_tree = scan_fs(dir)?;

    if is_update {
        let old_tree = store.load(Version::Latest)?;
        new_tree.update_from(&old_tree);
    }

    let estimate = new_tree.hash_estimate();
    let mut progress = Progress::new(estimate.files, estimate.bytes);
    new_tree.hash_update(dir, &mut progress);
    progress.flush();

    store.write_new(&new_tree, &BTreeMap::new())?;
    Ok(())
}
