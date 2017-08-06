// Errors in the weave code.

#[derive(Debug, error_chain)]
pub enum ErrorKind {
    Msg(String),

    #[error_chain(foreign)]
    Io(::std::io::Error),

    #[error_chain(foreign)]
    Parse(::std::num::ParseIntError),

    #[error_chain(foreign)]
    Serde(::serde_json::Error),
}