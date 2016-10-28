// Errors.

use escape;
use std::io;
use std::process::ExitStatus;

error_chain! {
    types {
        Error, ErrorKind, ChainErr, Result;
    }

    links {
        escape::EscapeError, escape::EscapeErrorKind, Escape;
    }

    foreign_links {
        io::Error, IoError;
    }

    errors {
        BkError(status: ExitStatus, msg: String) {
            description("Error running BitKeeper")
            display("Error running BitKeeper: {:?} ({:?}", status, msg)
        }
    }
}
