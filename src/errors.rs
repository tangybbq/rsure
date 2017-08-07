// Errors.

use escape;
use openssl;
use weave;
use std::io;
use std::process::ExitStatus;

error_chain! {
    types {
        Error, ErrorKind, ChainErr, Result;
    }

    links {
        Escape(escape::EscapeError, escape::EscapeErrorKind);
        Weave(weave::Error, weave::ErrorKind);
    }

    foreign_links {
        IoError(io::Error);
        OpensslError(openssl::error::ErrorStack);
        Parse(::std::num::ParseIntError);
    }

    errors {
        BkError(status: ExitStatus, msg: String) {
            description("Error running BitKeeper")
            display("Error running BitKeeper: {:?} ({:?}", status, msg)
        }
    }
}
