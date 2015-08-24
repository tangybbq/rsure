// Rsure library.

extern crate flate2;
extern crate libc;
extern crate openssl;
extern crate rustc_serialize;
extern crate time;

#[macro_use]
extern crate log;

use std::error;
use std::result;

pub use surefs::scan_fs;
pub use hashes::SureHash;
pub use suretree::SureTree;
pub use comp::{TreeCompare, TreeUpdate};
pub use show::show_tree;
pub use progress::Progress;

pub type Result<T> = result::Result<T, Box<error::Error + Send + Sync>>;

mod escape;
mod show;
mod suretree;
mod surefs;
mod hashes;
mod comp;
mod progress;
