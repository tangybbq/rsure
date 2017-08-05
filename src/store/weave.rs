//! SCCS-style delta weave stores.

use Result;
use SureTree;
use std::cell::RefCell;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::rc::Rc;

use super::{Store, StoreTags, Version};
use weave::{DeltaWriter, SimpleNaming, NamingConvention, NewWeave, Parser, Sink};

pub struct WeaveStore {
    naming: SimpleNaming,
}

impl WeaveStore {
    pub fn new<P: AsRef<Path>>(path: P, base: &str, compressed: bool) -> WeaveStore {
        WeaveStore {
            naming: SimpleNaming::new(path, base, "weave", compressed),
        }
    }
}

impl Store for WeaveStore {
    fn write_new(&self, tree: &SureTree, tags: &StoreTags) -> Result<()> {
        let itags = tags.iter().map(|(k,v)| (k.as_ref(), v.as_ref()));
        match get_last_delta(&self.naming) {
            Ok(base) => {
                let mut wv = DeltaWriter::new(&self.naming, itags, base)?;
                tree.save_to(&mut wv)?;
                wv.close()?;
                Ok(())
            }
            Err(_) => {
                // Create a new weave file.
                let mut wv = NewWeave::new(&self.naming, itags)?;

                tree.save_to(&mut wv)?;

                wv.close()?;
                Ok(())
            }
        }
    }

    fn load(&self, version: Version) -> Result<SureTree> {
        panic!("TODO")
    }
}

/// Get the delta number of the most recent delta.
fn get_last_delta(naming: &NamingConvention) -> Result<usize> {
    let fd = File::open(naming.main_file())?;
    let lines = BufReader::new(fd).lines();
    let parser = Parser::new(lines, Rc::new(RefCell::new(NullSink)), 1)?;
    let base = parser.get_header().deltas.iter()
        .map(|x| x.number)
        .max().expect("At least one delta in weave file");
    Ok(base)
}

// The null sink does nothing.
struct NullSink;

impl Sink for NullSink {}
