// Errors.

use failure;
use failure_derive::Fail;
use std::{process::ExitStatus, result};

pub type Result<T> = result::Result<T, Error>;
pub type Error = failure::Error;

#[derive(Fail, Debug)]
pub enum WeaveError {
    #[fail(display = "Error running BitKeeper: {:?}: {:?}", _0, _1)]
    BkError(ExitStatus, String),
}
