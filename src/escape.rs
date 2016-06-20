//! String escaping.
//!
//! Although filenames in Linux are commonly represented as UTF-8
//! sequences, there is no system requirement that this be the case.  As a
//! consequence, this means that it is possible for filenames in Linux to
//! not be valid UTF-8, and therefore not representable as strings.
//!
//! To prevent encoding problems, as well as to allow certain characters,
//! such as space, to separate tokens in the sure file format, we escape
//! some bytes in strings by replacing them with "=xx" where "xx" is the
//! lower-cased hex version of the string.  The range of valid characters
//! is fairly straightforward, including all of the printable characters
//! from '!' to '~' except for the '=', which is always escaped.  This
//! means, for example, that a 2-byte encoded UTF-8 sequence will expand to
//! take 6 bytes.

use std::io::prelude::*;

pub trait Escape {
    fn escaped(&self) -> String;
}

pub trait Unescape {
    fn unescape(&self) -> Result<Vec<u8>, EscapeError>;
}

error_chain! {
    types {
        EscapeError, EscapeErrorKind, EscapChainError, EscapeResult;
    }

    links {
    }

    foreign_links {
    }

    errors {
        InvalidHexCharacter(ch: u8) {
            description("Invalid hex character")
            display("Invalid hex character: {:?}", ch)
        }
        InvalidHexLength {
            description("Invalid hex length")
            display("Invalid hex length")
        }
    }
}

// The basic encoding converts a sequence of bytes into a string.
impl Escape for [u8] {
    fn escaped(&self) -> String {
        let mut result = vec![];
        for &ch in self.iter() {
            // TODO: Can be made more efficient.
            if b'!' <= ch && ch <= b'~' && ch != b'=' {
                result.push(ch);
            } else {
                write!(&mut result, "={:02x}", ch).unwrap();
            }
        }

        // TODO: String::from_utf8_unchecked(result)
        String::from_utf8(result).unwrap()
    }
}

impl Unescape for str {
    fn unescape(&self) -> Result<Vec<u8>, EscapeError> {
        // Will overestimate.
        let mut buf = Vec::with_capacity(self.len() / 2);
        let mut phase = 0;
        let mut tmp = 0;

        for byte in self.bytes() {
            if phase == 0 {
                if byte == b'=' {
                    phase = 1;
                } else {
                    buf.push(byte);
                }
            } else {
                tmp <<= 4;
                match byte {
                    b'A'...b'F' => tmp |= byte - b'A' + 10,
                    b'a'...b'f' => tmp |= byte - b'a' + 10,
                    b'0'...b'f' => tmp |= byte - b'0',
                    _ => return Err(EscapeErrorKind::InvalidHexCharacter(byte).into()),
                }
                phase += 1;
                if phase == 3 {
                    buf.push(tmp);
                    phase = 0;
                    tmp = 0;
                }
            }
        }

        if phase != 0 {
            return Err(EscapeErrorKind::InvalidHexLength.into());
        }

        Ok(buf)
    }
}

#[test]
fn test_unescape() {
    macro_rules! assert_error_kind {
        ( $expr:expr, $kind:pat ) => {
            match $expr.unwrap_err().into_kind() {
                $kind => (),
                e => panic!("Unexpected error kind: {:?} (want {})", e, stringify!($kind)),
            }
        }
    }

    assert_eq!("=00".unescape().unwrap(), vec![0]);
    assert_error_kind!("=00=0".unescape(), EscapeErrorKind::InvalidHexLength);
    assert_error_kind!("=00=".unescape(), EscapeErrorKind::InvalidHexLength);
    assert_error_kind!("=4g".unescape(), EscapeErrorKind::InvalidHexCharacter(b'g'));
}

#[test]
fn test_escape() {
    let buf: Vec<u8> = (0u32..256).map(|i| i as u8).collect();
    let text = (&buf[..]).escaped();
    assert_eq!(text.unescape().unwrap(), buf);
}
