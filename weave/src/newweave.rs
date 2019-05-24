//! Writer for new weaves

use failure::err_msg;
use std::{
    collections::BTreeMap,
    fs::rename,
    io::{self, Write},
    mem::replace,
};

use crate::{header::Header, NamingConvention, Result, WriterInfo};

/// A builder for a new weave file.  The data should be written as a writer.  Closing the weaver
/// will finish up the write and move the new file into place.  If the weaver is just dropped, the
/// file will not be moved into place.
pub struct NewWeave<'n> {
    naming: &'n dyn NamingConvention,
    temp: Option<WriterInfo>,
}

impl<'n> NewWeave<'n> {
    pub fn new<'a, 'b, I>(nc: &dyn NamingConvention, tags: I) -> Result<NewWeave>
    where
        I: Iterator<Item = (&'a str, &'b str)>,
    {
        let mut writeinfo = nc.new_temp()?;

        let mut ntags = BTreeMap::new();
        for (k, v) in tags {
            ntags.insert(k.to_owned(), v.to_owned());
        }
        let mut header: Header = Default::default();
        let delta = header.add(ntags)?;
        header.write(&mut writeinfo.writer)?;
        writeln!(&mut writeinfo.writer, "\x01I {}", delta)?;

        Ok(NewWeave {
            naming: nc,
            temp: Some(writeinfo),
        })
    }

    pub fn close(mut self) -> Result<()> {
        let temp = replace(&mut self.temp, None);
        let name = match temp {
            Some(mut wi) => {
                writeln!(&mut wi.writer, "\x01E 1")?;
                wi.name
            }
            None => return Err(err_msg("NewWeave already closed")),
        };
        let _ = rename(self.naming.main_file(), self.naming.backup_file());
        rename(name, self.naming.main_file())?;
        Ok(())
    }
}

impl<'n> Write for NewWeave<'n> {
    // Write the data out, just passing it through to the underlying file write.  We assume the
    // last line is terminated, or the resulting weave will be invalid.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.temp
            .as_mut()
            .expect("Attempt to write to NewWeave that is closed")
            .writer
            .write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.temp
            .as_mut()
            .expect("Attempt to flush NewWeave that is closed")
            .writer
            .flush()
    }
}

#[test]
#[ignore]
fn try_tag() {
    use crate::SimpleNaming;
    let mut tags = BTreeMap::new();
    tags.insert("name".to_owned(), "initial revision".to_owned());
    // Add a whole bunch of longer tags to show it works.
    for i in 1..100 {
        tags.insert(format!("key{}", i), format!("This is the {}th value", i));
    }
    let nc = SimpleNaming::new(".", "tags", "weave", false);
    let t2 = tags.iter().map(|(k, v)| (k.as_ref(), v.as_ref()));
    let mut wr = NewWeave::new(&nc, t2).unwrap();
    writeln!(&mut wr, "This is the only line in the file").unwrap();
    wr.close().unwrap();
}
