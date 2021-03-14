// Surefile store

use crate::{Error, Result, SureNode};
use chrono::{DateTime, Utc};
use log::info;
use std::{
    collections::BTreeMap,
    io::{BufRead, Write},
    path::Path,
};

mod weave;

pub use self::weave::WeaveStore;

/// Tags are just key/value pairs.  Both key and value should be printable strings.
pub type StoreTags = BTreeMap<String, String>;

/// Something that can store and retrieve SureTrees.
pub trait Store {
    /// Retrieve the available versions, in the store.  These should be listed, newest first.
    fn get_versions(&self) -> Result<Vec<StoreVersion>>;

    /// Load the specified version, returning an iterator over the nodes.
    fn load_iter(&self, version: Version) -> Result<Box<dyn Iterator<Item = Result<SureNode>>>>;

    /// Create a temporary storage location.
    fn make_temp(&self) -> Result<Box<dyn TempFile + '_>>;

    /// Create a writer for a new version.
    fn make_new(&self, tags: &StoreTags) -> Result<Box<dyn StoreWriter + '_>>;
}

/// A TempFile is a temporary storage location that can be written to, and
/// then committed as a new version, or discarded entirely if it is
/// dropped.
/// Typical usage patterns are:
/// - Write to the file, turn into a reader to reread the data.  Will be
///   deleted on drop.
/// - Write to the file, turn into a loader which can make multiple
///   readers.  Will be deleted on drop.
/// - Write to the file, which can then be committed.  File will be
///   deleted, but data merged into the latest version in the store.
pub trait TempFile<'a>: Write {
    fn into_loader(self: Box<Self>) -> Result<Box<dyn TempLoader + 'a>>;

    // Close the file, returning a TempCleaner that will clean up the file
    // when it is dropped.  Significantly, this has no lifetime
    // dependencies.
    fn into_cleaner(self: Box<Self>) -> Result<Box<dyn TempCleaner>>;
}

/// A temp file that can spawn multiple loaders.
pub trait TempLoader {
    /// Open the temp file, and return a reader on it.
    fn new_loader(&self) -> Result<Box<dyn BufRead>>;

    /// Return the name of the temp file.
    fn path_ref(&self) -> &Path;

    // Close the file, returning a TempCleaner that will clean up the file
    // when it is dropped.  Significantly, this has no lifetime
    // dependencies.
    fn into_cleaner(self: Box<Self>) -> Result<Box<dyn TempCleaner>>;
}

/// A Writer for adding a new version.
pub trait StoreWriter<'a>: Write {
    /// All data has been written, commit this as a new version.
    fn commit(self: Box<Self>) -> Result<()>;
}

pub trait TempCleaner {}

/// Indicator of which version of sure data to load.
#[derive(Clone, Debug)]
pub enum Version {
    Latest,
    Prior,
    Tagged(String),
}

impl Version {
    /// Retrieve this version as a number, or none if that makes no sense
    /// (either it is `Latest`, `Prior`, or the textual version is not an
    /// integer).
    pub fn numeric(&self) -> Option<usize> {
        match self {
            Version::Latest | Version::Prior => None,
            Version::Tagged(text) => text.parse().ok(),
        }
    }
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
        return Ok(Box::new(WeaveStore::new(p.to_path_buf(), "2sure", true)));
    }

    // Otherwise, try to get the parent.  If it seems to be empty, use the current directory as the
    // path.
    let dir = match p.parent() {
        None => return Err(Error::UnknownDirectory),
        Some(dir) => {
            if dir.as_os_str().is_empty() {
                Path::new(".")
            } else {
                dir
            }
        }
    };

    if !dir.is_dir() {
        return Err(Error::FileNotInDirectory);
    }

    let base = match p.file_name() {
        Some(name) => name,
        None => return Err(Error::PathMissingFinalFile),
    };
    let base = match base.to_str() {
        Some(name) => name,
        None => panic!("Path came from string, yet is no longer UTF-8"),
    };

    let (base, compressed) = if let Some(core_name) = base.strip_suffix(".gz") {
        (core_name, true)
    } else {
        (base, false)
    };

    // Check for weave format.
    if let Some(base) = base.strip_suffix(".weave") {
        return Ok(Box::new(WeaveStore::new(dir, base, compressed)));
    }

    // Strip off known suffixes.
    let base = if base.ends_with(".dat") || base.ends_with(".bak") {
        &base[..base.len() - 4]
    } else {
        base
    };

    Ok(Box::new(WeaveStore::new(dir, base, compressed)))
}
