//! Rsure is a set of utilities for capturing information about files, and later verifying it is
//! still true.
//!
//! The easiest way to use Rsure is to build the `rsure` executable contained in this crate.  This
//! program allows you to use most of the functionality of the crate.
//!
//! However, it is also possible to use the crate programmatically.  At the top level of the crate
//! as some utility functions for the most common operations.
//!
//! For example, to scan a directory or do an update use `update`.
//!
//! This example makes use of several of the building blocks necessary to use the store.  First is
//! the store itself.  `parse_store` is able to decode options that are passed to the command line.
//! it is also possible to build a `store::Plain` store directly.
//!
//! Next are the tags for the snapshot.  Generally, this should hold some kind of information about
//! the snapshot itself.  For the `Plain` store, it can be just an empty map.  Other store types
//! may require certain tags to be present.

extern crate chrono;
extern crate flate2;
extern crate libc;
extern crate openssl;
extern crate regex;
extern crate rustc_serialize;
extern crate time;
extern crate weave;

#[macro_use]
extern crate log;

#[macro_use] extern crate failure;
#[macro_use] extern crate failure_derive;

use std::path::Path;

pub use surefs::scan_fs;
pub use hashes::SureHash;
pub use suretree::SureTree;
pub use comp::{TreeCompare, TreeUpdate};
pub use compvisit::{CompareVisitor, CompareType, CompareAction, PrintVisitor, stdout_visitor,
                    stderr_visitor};
pub use show::show_tree;
pub use progress::Progress;

pub use errors::{Error, WeaveError, Result};

pub use store::{StoreTags, Store, StoreVersion, Version, parse_store, BkSureFile, BkStore,
                bk_setup};

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

// Some common operations, abstracted here.

/// Perform an update scan, using the given store.
///
/// If 'update' is true, use the hashes from a previous run, otherwise perform a fresh scan.
/// Depending on the [`Store`] type, the tags may be kept, or ignored.
///
/// [`Store`]: trait.Store.html
///
/// A simple example:
///
/// ```rust
/// # use std::error::Error;
/// #
/// # fn try_main() -> Result<(), Box<Error>> {
/// let mut tags = rsure::StoreTags::new();
/// tags.insert("name".into(), "sample".into());
/// let store = rsure::parse_store("2sure.dat.gz")?;
/// rsure::update(".", &*store, false, &tags)?;
/// #     Ok(())
/// # }
/// #
/// # fn main() {
/// #     try_main().unwrap();
/// # }
/// ```
pub fn update<P: AsRef<Path>>(
    dir: P,
    store: &Store,
    is_update: bool,
    tags: &StoreTags,
) -> Result<()> {
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

    store.write_new(&new_tree, tags)?;
    Ok(())
}
