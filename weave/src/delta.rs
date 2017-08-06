//! Add a delta to a weave file.

use regex::Regex;
use std::collections::BTreeMap;
use std::fs::{rename, remove_file};
use std::mem::replace;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use header::Header;
use NamingConvention;
use Parser;
use Sink;
use Result;
use WriterInfo;

/// A DeltaWriter is used to write a new delta.  Data should be written to the writer, and then the
/// `close` method called to update the weave file with the new delta.
pub struct DeltaWriter<'n> {
    naming: &'n NamingConvention,

    // Where the temporary file will be written.
    temp: Option<WriterInfo>,

    // The base delta.
    base: usize,

    // The new delta.
    new_delta: usize,

    // The name of the file with the base written to it.
    base_name: PathBuf,

    // The regex for parsing diff output.
    diff_re: Regex,

    // The header to be written for the new delta.
    header: Header,
}

impl<'n> DeltaWriter<'n> {
    /// Construct a writer for a new delta.  The naming convention and the tags set where the names
    /// will be written, and what tags will be associated with the convention.  The `base` is the
    /// existing delta that the change should be based on.
    pub fn new<'a, 'b, I>(nc: &NamingConvention, tags: I, base: usize) -> Result<DeltaWriter>
        where I: Iterator<Item=(&'a str, &'b str)>
    {
        // Copy the tags, making sure there is a "name", which is used to index.
        // TODO: Ensure that "name" is unique among the existing deltas.
        let mut ntags = BTreeMap::new();
        for (k, v) in tags {
            ntags.insert(k.to_owned(), v.to_owned());
        }
        if !ntags.contains_key("name") {
            return Err("DeltaWriter does not contain a tag \"name\"".into());
        }

        // Extract the base delta to a file.

        let (base_name, base_file) = nc.temp_file()?;
        let mut header = {
            let dsync = RevWriter { dest: BufWriter::new(base_file) };
            let mut parser = Parser::new(nc, dsync, base)?;
            match parser.parse_to(0) {
                Ok(0) => (),
                Ok(_) => panic!("Unexpected stop of parser"),
                Err(e) => return Err(e),
            }
            parser.into_header()
        };
        let new_delta = header.add(ntags)?;

        let (new_name, new_file) = nc.temp_file()?;
        let new_info = WriterInfo {
            name: new_name,
            writer: Box::new(BufWriter::new(new_file)),
        };

        Ok(DeltaWriter {
            naming: nc,
            temp: Some(new_info),
            base: base,
            new_delta: new_delta,
            base_name: base_name,
            diff_re: Regex::new(r"^(\d+)(,(\d+))?([acd]).*$").unwrap(),
            header: header,
        })
    }

    pub fn close(mut self) -> Result<()> {
        // Close the temporary file, getting its name.
        let temp = replace(&mut self.temp, None);
        let temp_name = match temp {
            Some(mut wi) => {
                wi.writer.flush()?;
                drop(wi.writer);
                wi.name
            }
            None => return Err("DeltaWriter already closed".into()),
        };

        let tweave_info = self.naming.new_temp()?;

        // Invoke diff on the files.
        let mut child = Command::new("diff")
            .arg(self.base_name.as_os_str())
            .arg(temp_name.as_os_str())
            .stdout(Stdio::piped())
            .spawn()?;

        {
            let lines = BufReader::new(child.stdout.as_mut().unwrap()).lines();
            let weave_write = WeaveWriter { dest: tweave_info.writer };
            let mut parser = Parser::new(self.naming, weave_write, self.base)?;

            let weave_write = parser.get_sink();

            self.header.write(&mut weave_write.borrow_mut().dest)?;

            let mut is_done = false;
            let mut is_adding = false;

            for line in lines {
                let line = line?;
                match self.diff_re.captures(&line) {
                    Some(cap) => {
                        // If adding, this completes the add.
                        if is_adding {
                            weave_write.borrow_mut().end(self.new_delta)?;
                            is_adding = false;
                        }

                        let left = cap.get(1).unwrap().as_str().parse::<usize>().unwrap();
                        let right = match cap.get(3) {
                            None => left,
                            Some(r) => r.as_str().parse().unwrap(),
                        };
                        let cmd = cap.get(4).unwrap().as_str().chars().next().unwrap();

                        if cmd == 'd' || cmd == 'c' {
                            // These include deletions.
                            match parser.parse_to(left)? {
                                0 => return Err("Unexpected eof".into()),
                                n if n == left => (),
                                _ => panic!("Unexpected parse result"),
                            }
                            weave_write.borrow_mut().delete(self.new_delta)?;
                            match parser.parse_to(right + 1) {
                                Ok(0) => is_done = true,
                                Ok(n) if n == right + 1 => (),
                                Ok(_) => panic!("Unexpected parse result"),
                                Err(e) => return Err(e),
                            }
                            weave_write.borrow_mut().end(self.new_delta)?;
                        } else {
                            match parser.parse_to(right + 1) {
                                Ok(0) => is_done = true,
                                Ok(n) if n == right + 1 => (),
                                Ok(_) => panic!("Unexpected parse result"),
                                Err(e) => return Err(e),
                            }
                        }

                        if cmd == 'c' || cmd == 'a' {
                            weave_write.borrow_mut().insert(self.new_delta)?;
                            is_adding = true;
                        }

                        continue;
                    },
                    None => (),
                }

                match line.chars().next() {
                    None => panic!("Unexpected blank line in diff"),
                    Some('<') => continue,
                    Some('-') => continue,
                    Some('>') => {
                        // Add lines should just be written as-is.
                        weave_write.borrow_mut().plain(&line[2..], true)?;
                    }
                    Some(_) => panic!("Unexpected diff line: {:?}", line),
                }
            }

            if is_adding {
                weave_write.borrow_mut().end(self.new_delta)?;
            }

            if !is_done {
                match parser.parse_to(0) {
                    Ok(0) => (),
                    Ok(_) => panic!("Unexpected non-eof"),
                    Err(e) => return Err(e),
                }
            }
        }

        match child.wait()?.code() {
            None => return Err("diff killed by signal".into()),
            Some(0) => (), // No diffs
            Some(1) => (), // Normal with diffs
            Some(n) => return Err(format!("diff returned error status: {}", n).into()),
        }

        // Now that is all done, clean up the temp files, and cycle the backup.
        let _ = rename(self.naming.main_file(), self.naming.backup_file());
        rename(tweave_info.name, self.naming.main_file())?;
        remove_file(&self.base_name)?;
        remove_file(&temp_name)?;

        Ok(())
    }
}

impl <'n> Write for DeltaWriter<'n> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.temp.as_mut()
            .expect("Attempt to write to DeltaWriter that is closed")
            .writer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.temp.as_mut()
            .expect("Attempt to flush DeltaWriter that is closed")
            .writer.flush()
    }
}

struct RevWriter<W: Write> {
    dest: BufWriter<W>,
}

impl<W: Write> Sink for RevWriter<W> {
    fn plain(&mut self, text: &str, keep: bool) -> Result<()> {
        if !keep {
            return Ok(());
        }

        writeln!(&mut self.dest, "{}", text)?;
        Ok(())
    }
}

/// The weave writer writes out the contents of a weave to a file.
struct WeaveWriter<W: Write> {
    dest: W,
}

impl <W: Write> Sink for WeaveWriter<W> {
    fn insert(&mut self, delta: usize) -> Result<()> {
        writeln!(&mut self.dest, "\x01I {}", delta)?;
        Ok(())
    }
    fn delete(&mut self, delta: usize) -> Result<()> {
        writeln!(&mut self.dest, "\x01D {}", delta)?;
        Ok(())
    }
    fn end(&mut self, delta: usize) -> Result<()> {
        writeln!(&mut self.dest, "\x01E {}", delta)?;
        Ok(())
    }
    fn plain(&mut self, text: &str, _keep: bool) -> Result<()> {
        writeln!(&mut self.dest, "{}", text)?;
        Ok(())
    }
}
