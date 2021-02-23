// Errors in the weave code.

use thiserror::Error;
use std::{
    io,
    result,
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("I/O Error")]
    Io(#[from] io::Error),
    #[error("Json error")]
    Json(#[from] serde_json::Error),
    #[error("Parsing Error")]
    Parse(#[from] std::num::ParseIntError),
    #[error("tag \"name\" missing")]
    NameMissing,
    #[error("already closed")]
    AlreadyClosed,
    #[error("unexpected end of weave file")]
    UnexpectedEof,
    #[error("weave file appears empty")]
    EmptyWeave,
    #[error("diff error status {0}")]
    DiffError(i32),
    #[error("diff killed by signal")]
    DiffKilled,
}

pub type Result<T> = result::Result<T, Error>;
