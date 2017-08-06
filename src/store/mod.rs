// Surefile store

use ::Result;
use ::SureTree;
use std::collections::BTreeMap;
use std::path::Path;

mod plain;
mod bk;
mod weave;

pub use self::plain::Plain;
pub use self::bk::{BkSureFile, BkStore, bk_setup};
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
}

/// Indicator of which version of sure data to load.
#[derive(Clone, Copy, Debug)]
pub enum Version {
    Latest,
    Prior,
}

/// Parse a command line specified path to determine the parameters and type of store desired.  The
/// path can be the path to a directory.  In this case, look at possible filenames to determine the
/// other parameters.  The path can also give a filename of one of the surefiles, and we will
/// derive the name information from that.
pub fn parse_store(text: &str) -> Result<Box<Store>> {
    // First determine if this path is a directory.
    let p = Path::new(text);
    info!("Parsing: {:?}", p);

    // If we're given an existing directory, construct a store directly from it.
    // TODO: Look in the directory to see what might be there.
    if p.is_dir() {
        // Check for BK directory, and reject without explicit name.
        if p.join(".bk").is_dir() {
            return Err("Store appears to be a Bitkeeper dir, specify full filename".into());
        }

        return Ok(Box::new(Plain {
            path: p.to_path_buf(),
            base: "2sure".to_string(),
            compressed: true,
        }))
    }

    // Otherwise, try to get the parent.  If it seems to be empty, use the current directory as the
    // path.
    let dir = match p.parent() {
        None => return Err("Unknown directory specified".into()),
        Some(dir) => {
            if dir.as_os_str().is_empty() {
                Path::new(".")
            } else {
                dir
            }
        },
    };

    if !dir.is_dir() {
        return Err("File is not in a directory".into());
    }

    let base = match p.file_name() {
        Some(name) => name,
        None => return Err("Path does not have a final file component".into()),
    };
    let base = match base.to_str() {
        Some(name) => name,
        None => panic!("Path came from string, yet is no longer UTF-8"),
    };

    let (base, compressed) = if base.ends_with(".gz") {
        (&base[..base.len()-3], true)
    } else {
        (base, false)
    };

    // Check for weave format.
    if base.ends_with(".weave") {
        let base = &base[..base.len()-6];
        return Ok(Box::new(WeaveStore::new(dir, base, compressed)));
    }

    // Strip off known suffixes.
    let base = if base.ends_with(".dat") || base.ends_with(".bak") {
        &base[..base.len()-4]
    } else {
        base
    };

    // Check for bitkeeper.
    if dir.join(".bk").is_dir() {
        if compressed {
            return Err("Bitkeeper names should not be compressed, remove .gz suffix".into());
        }

        return Ok(Box::new(BkStore::new(dir, base)));
    }

    Ok(Box::new(Plain {
        path: dir.to_path_buf(),
        base: base.to_string(),
        compressed: compressed,
    }))
}