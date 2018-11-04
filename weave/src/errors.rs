// Errors in the weave code.

use failure;
use std::result;

pub type Result<T> = result::Result<T, Error>;
pub type Error = failure::Error;
