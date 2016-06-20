// Errors.

use std::io;

error_chain! {
    types {
        Error, ErrorKind, ChainErr, Result;
    }

    links {
    }

    foreign_links {
        io::Error, IoError, "I/O Error";
    }

    errors {
    }
}
