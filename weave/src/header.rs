//! Weave file information.
//!
//! The information about each weave file is stored in a header, as the first line of the file.

use chrono::{DateTime, Utc};
use failure::err_msg;
use serde_derive::{Deserialize, Serialize};
use serde_json;
use std::{collections::BTreeMap, io::Write};

use crate::Result;

/// The header placed at the beginning of the each weave file.
#[derive(Clone, Serialize, Deserialize)]
pub struct Header {
    pub version: usize,
    pub deltas: Vec<DeltaInfo>,
}

/// Information about a single delta.
#[derive(Clone, Serialize, Deserialize)]
pub struct DeltaInfo {
    /// A tag giving the name for this particular delta.  Should be unique across all deltas.
    pub name: String,
    /// The delta number.  A unique integer that identifies this delta in the woven data below.
    pub number: usize,
    /// Arbitrary tags the user has asked to be stored with this delta.
    pub tags: BTreeMap<String, String>,
    /// A time stamp when this delta was added.
    pub time: DateTime<Utc>,
}

const THIS_VERSION: usize = 1;

impl Default for Header {
    fn default() -> Header {
        Header {
            version: THIS_VERSION,
            deltas: vec![],
        }
    }
}

impl Header {
    /// Decode from the first line of the file.
    pub fn decode(line: &str) -> Result<Header> {
        if line.starts_with("\x01t") {
            Ok(serde_json::from_str(&line[2..])?)
        } else {
            // This probably comes from an sccs file.
            Ok(Header {
                version: 0,
                deltas: vec![],
            })
        }
    }

    /// Add a delta to this header.  Returns the delta number to be used.
    pub fn add(&mut self, mut tags: BTreeMap<String, String>) -> Result<usize> {
        let name = if let Some(name) = tags.remove("name") {
            name
        } else {
            return Err(err_msg("No \"name\" tag given"));
        };

        let next_delta = self.deltas.iter().map(|x| x.number).max().unwrap_or(0) + 1;

        self.deltas.push(DeltaInfo {
            name,
            number: next_delta,
            tags,
            time: Utc::now(),
        });

        Ok(next_delta)
    }

    /// Write the header to the writer, as the first line.
    pub fn write<W: Write>(&self, mut wr: &mut W) -> Result<()> {
        write!(&mut wr, "\x01t")?;
        serde_json::to_writer(&mut wr, &self)?;
        writeln!(&mut wr)?;
        Ok(())
    }
}
