// Errors.

use thiserror::Error;
use std::{/*process::ExitStatus,*/ result};

pub type Result<T> = result::Result<T, Error>;
#[derive(Error, Debug)]
pub enum Error {
    #[error("weave error")]
    Weave(#[from] weave::Error),

    #[error("I/O Error {0:?}")]
    Io(#[from] std::io::Error),

    #[error("OpenSSL error: {0:?}")]
    OpenSsl(#[from] openssl::error::ErrorStack),
    #[error("Int parse error: {0:?}")]
    IntParse(#[from] std::num::ParseIntError),

    #[error("Root must be a directory")]
    RootMustBeDir,
    #[error("Unknown directory specified")]
    UnknownDirectory,
    #[error("File not in directory")]
    FileNotInDirectory,
    #[error("Path missing final file component")]
    PathMissingFinalFile,

    // Errors from comparison.
    #[error("empty left iterator")]
    EmptyLeftIterator,
    #[error("empty right iterator")]
    EmptyRightIterator,
    #[error("Unexpected node in left tree")]
    UnexpectedLeftNode,
    #[error("Unexpected node in right tree")]
    UnexpectedRightNode,
    #[error("Incorrect name of root tree")]
    IncorrectName,

    #[error("Unexpected line: {0:?}, expect {1:?}")]
    UnexpectedLine(String, String),
    #[error("Error reading surefile: {0:?}")]
    SureFileError(std::io::Error),
    #[error("Unexpected eof on surefile")]
    SureFileEof,
    #[error("Truncated surefile")]
    TruncatedSurefile,
    #[error("Invalid surefile line start: {0:?}")]
    InvalidSurefileChar(char),

    #[error("Sql error: {0:?}")]
    Sql(#[from] rusqlite::Error),
    // For one case that needs to be written to be able to move the error.
    #[error("Sql error: {0}")]
    WrappedSql(String),
    #[error("Hash error: {0:?}")]
    Hash(String),
    #[error("mpsc error: {0:?}")]
    Mpsc(#[from] std::sync::mpsc::RecvError),
}

/*
#[derive(Fail, Debug)]
pub enum WeaveError {
    #[fail(display = "Error running BitKeeper: {:?}: {:?}", _0, _1)]
    BkError(ExitStatus, String),
}
*/
