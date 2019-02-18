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

#![warn(bare_trait_objects)]

use std::{
    fs::File,
    path::Path,
};

pub use crate::{
    errors::{Error, Result, WeaveError},
    hashes::Estimate,
    node::{
        HashCombiner,
        HashUpdater,
        NodeWriter,
        ReadIterator,
        Source,
        SureNode,
        compare_trees,
        fs,
        load_from,
    },
    progress::{log_init, Progress},
    show::show_tree,
    store::{
        Store,
        StoreTags,
        StoreVersion,
        TempLoader,
        Version,
        parse_store,
    },
    surefs::scan_fs,
    suretree::SureTree,
};

mod errors;
mod escape;
mod hashes;
pub mod node;
mod progress;
mod show;
mod store;
mod surefs;
mod suretree;

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
    store: &dyn Store,
    is_update: bool,
    tags: &StoreTags,
) -> Result<()> {
    let dir = dir.as_ref();

    let mut estimate = Estimate { files: 0, bytes: 0 };
    let tmp = if is_update {
        // In update mode, first tmp file is just the scan.
        let scan_temp = {
            let mut tmp = store.make_temp()?;
            let src = fs::scan_fs(dir)?;
            node::save_to(&mut tmp, src)?;
            tmp
        }.into_loader()?;

        let latest = store.load_iter(Version::Latest)?;

        let tmp = {
            let mut tmp = store.make_temp()?;
            let loader = Loader(&*scan_temp);
            let combiner = HashCombiner::new(latest, loader.iter()?)?
                .inspect(|node| {
                    match node {
                        Ok(n @ SureNode::File { .. }) => {
                            if n.needs_hash() {
                                estimate.files += 1;
                                estimate.bytes += n.size();
                            }
                        }
                        _ => (),
                    }
                });
            node::save_to(&mut tmp, combiner)?;
            tmp
        };

        tmp
    } else {
        let mut tmp = store.make_temp()?;
        let src = fs::scan_fs(dir)?
            .inspect(|node| {
                match node {
                    // TODO: This is only correct if this is not an update.
                    Ok(n @ SureNode::File { .. }) => {
                        if n.needs_hash() {
                            estimate.files += 1;
                            estimate.bytes += n.size();
                        }
                    }
                    _ => (),
                }
            });
        node::save_to(&mut tmp, src)?;
        tmp
    }.into_loader()?;

    // TODO: If this is an update, pull in hashes from the old version.

    // Update any missing hashes.
    let loader = Loader(&*tmp);
    let hu = HashUpdater::new(loader, store);
    // TODO: This will panic on non-unicode directories.
    let hm = hu.compute_parallel(dir.to_str().unwrap(), &estimate)?;
    let mut tmp2 = store.make_new(tags)?;
    hm.merge(&mut NodeWriter::new(&mut tmp2)?)?;

    tmp2.commit()?;
/*
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
*/
    Ok(())
}

struct Loader<'a>(&'a dyn TempLoader);

impl<'a> Source for Loader<'a> {
    fn iter(&self) -> Result<Box<dyn Iterator<Item = Result<SureNode>> + Send>> {
        let rd = File::open(self.0.path_ref())?;
        Ok(Box::new(load_from(rd)?))
    }
}
