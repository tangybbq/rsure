// Errors in the weave code.

#[derive(Debug, error_chain)]
pub enum ErrorKind {
    Msg(String),

    #[error_chain(foreign)]
    Io(::std::io::Error),
}
