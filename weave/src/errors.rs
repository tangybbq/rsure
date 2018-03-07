// Errors in the weave code.

use std::result;
use failure;

pub type Result<T> = result::Result<T, Error>;
pub type Error = failure::Error;
