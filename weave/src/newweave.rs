//! Writer for new weaves

use std::fs::rename;
use std::mem::replace;
use std::io::{self, Write};

use Result;
use NamingConvention;
use WriterInfo;

/// A builder for a new weave file.  The data should be written as a writer.  Closing the weaver
/// will finish up the write and move the new file into place.  If the weaver is just dropped, the
/// file will not be moved into place.
pub struct NewWeave<'n> {
    naming: &'n NamingConvention,
    temp: Option<WriterInfo>,
}

impl<'n> NewWeave<'n> {
    pub fn new<'a, 'b, I>(nc: &NamingConvention, tags: I) -> Result<NewWeave>
        where I: Iterator<Item=(&'a str, &'b str)>
    {
        let (temp, mut file) = nc.temp_file()?;
        for (k, v) in tags {
            writeln!(&mut file, "\x01t {}={}", k, v)?;
        }
        writeln!(&mut file, "\x01I 1")?;

        Ok(NewWeave {
            naming: nc,
            temp: Some(WriterInfo {
                name: temp,
                file: file,
            }),
        })
    }

    pub fn close(mut self) -> Result<()> {
        let temp = replace(&mut self.temp, None);
        let name = match temp {
            Some(mut wi) => {
                writeln!(&mut wi.file, "\x01E 1")?;
                wi.name
            }
            None => return Err("NewWeave already closed".into()),
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
        self.temp.as_mut()
            .expect("Attempt to write to NewWeave that is closed")
            .file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.temp.as_mut()
            .expect("Attempt to flush NewWeave that is closed")
            .file.flush()
    }
}
