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

use std::error;
use std::fmt;
use std::io::prelude::*;

pub trait Escape {
    fn escaped(&self) -> String;
}

pub trait Unescape {
    fn unescape(&self) -> Result<Vec<u8>, EscapeError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EscapeError {
    /// The input contained a '=' escape with an invalid hex character.
    InvalidHexCharacter,
    /// The input contained a '=' escape with insufficient following
    /// characters.
    InvalidHexLength,
}

impl error::Error for EscapeError {
    fn description(&self) -> &str {
        match *self {
            EscapeError::InvalidHexCharacter => "Invalid hex character",
            EscapeError::InvalidHexLength => "Invalid length following '='",
        }
    }
}

impl fmt::Display for EscapeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self, f)
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
                    _ => return Err(EscapeError::InvalidHexCharacter),
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
            return Err(EscapeError::InvalidHexLength);
        }

        Ok(buf)
    }
}

#[test]
fn test_unescape() {
    assert_eq!("=00".unescape(), Ok(vec![0]));
    assert_eq!("=00=0".unescape(), Err(EscapeError::InvalidHexLength));
    assert_eq!("=00=".unescape(), Err(EscapeError::InvalidHexLength));
    assert_eq!("=4g".unescape(), Err(EscapeError::InvalidHexCharacter));
}

#[test]
fn test_escape() {
    let buf: Vec<u8> = (0u32..256).map(|i| i as u8).collect();
    let text = (&buf[..]).escaped();
    assert_eq!(text.unescape().unwrap(), buf);
}
