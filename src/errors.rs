// Errors.

use escape;
use std::io;

error_chain! {
    types {
        Error, ErrorKind, ChainErr, Result;
    }

    links {
        escape::EscapeError, escape::EscapeErrorKind, Escape;
    }

    foreign_links {
        io::Error, IoError, "I/O Error";
    }

    errors {
    }
}
