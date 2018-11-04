// Surefile store

use chrono::{DateTime, Utc};
use crate::Result;
use crate::SureTree;
use failure::err_msg;
use log::{info, log};
use std::collections::BTreeMap;
use std::path::Path;

mod bk;
mod plain;
mod weave;

pub use self::bk::{bk_setup, BkStore, BkSureFile};
pub use self::plain::Plain;
pub use self::weave::WeaveStore;

/// Tags are just key/value pairs.  Both key and value should be printable strings.
pub type StoreTags = BTreeMap<String, String>;

/// Something that can store and retrieve SureTrees.
pub trait Store {
    /// Write a new SureTree to the store.  The store may write the tags in the version to help
    /// identify information about what was captured.
    fn write_new(&self, tree: &SureTree, tags: &StoreTags) -> Result<()>;

    /// Attempt to load a sure version, based on the descriptor given.
    fn load(&self, version: Version) -> Result<SureTree>;

    /// Retrieve the available versions, in the store.  These should be listed, newest first.
    fn get_versions(&self) -> Result<Vec<StoreVersion>>;
}

/// Indicator of which version of sure data to load.
#[derive(Clone, Debug)]
pub enum Version {
    Latest,
    Prior,
    Tagged(String),
}

/// Information about a given version in the store.
#[derive(Clone, Debug)]
pub struct StoreVersion {
    /// A descriptive name.  May be the "name" tag given when this version was created.
    pub name: String,
    /// A timestamp of when the version was made.
    pub time: DateTime<Utc>,
    /// The identifier for this version.
    pub version: Version,
}

/// Parse a command line specified path to determine the parameters and type of store desired.  The
/// path can be the path to a directory.  In this case, look at possible filenames to determine the
/// other parameters.  The path can also give a filename of one of the surefiles, and we will
/// derive the name information from that.
pub fn parse_store(text: &str) -> Result<Box<dyn Store>> {
    // First determine if this path is a directory.
    let p = Path::new(text);
    info!("Parsing: {:?}", p);

    // If we're given an existing directory, construct a store directly from it.
    // TODO: Look in the directory to see what might be there.
    if p.is_dir() {
        // Check for BK directory, and reject without explicit name.
        if p.join(".bk").is_dir() {
            return Err(err_msg(
                "Store appears to be a Bitkeeper dir, specify full filename",
            ));
        }

        return Ok(Box::new(Plain {
            path: p.to_path_buf(),
            base: "2sure".to_string(),
            compressed: true,
        }));
    }

    // Otherwise, try to get the parent.  If it seems to be empty, use the current directory as the
    // path.
    let dir = match p.parent() {
        None => return Err(err_msg("Unknown directory specified")),
        Some(dir) => {
            if dir.as_os_str().is_empty() {
                Path::new(".")
            } else {
                dir
            }
        }
    };

    if !dir.is_dir() {
        return Err(err_msg("File is not in a directory"));
    }

    let base = match p.file_name() {
        Some(name) => name,
        None => return Err(err_msg("Path does not have a final file component")),
    };
    let base = match base.to_str() {
        Some(name) => name,
        None => panic!("Path came from string, yet is no longer UTF-8"),
    };

    let (base, compressed) = if base.ends_with(".gz") {
        (&base[..base.len() - 3], true)
    } else {
        (base, false)
    };

    // Check for weave format.
    if base.ends_with(".weave") {
        let base = &base[..base.len() - 6];
        return Ok(Box::new(WeaveStore::new(dir, base, compressed)));
    }

    // Strip off known suffixes.
    let base = if base.ends_with(".dat") || base.ends_with(".bak") {
        &base[..base.len() - 4]
    } else {
        base
    };

    // Check for bitkeeper.
    if dir.join(".bk").is_dir() {
        if compressed {
            return Err(err_msg(
                "Bitkeeper names should not be compressed, remove .gz suffix",
            ));
        }

        return Ok(Box::new(BkStore::new(dir, base)));
    }

    Ok(Box::new(WeaveStore::new(dir, base, compressed)))
}
