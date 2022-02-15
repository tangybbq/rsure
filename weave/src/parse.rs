//! Weave parsing

use crate::{header::Header, Error, NamingConvention, Result};
use flate2::read::GzDecoder;
use log::info;
use std::{
    cell::RefCell,
    fs::File,
    io::{BufRead, BufReader, Lines, Read},
    mem,
    rc::Rc,
};

/// A Sink is a place that a parsed weave can be sent to.  The insert/delete/end commands match
/// those in the weave file, and `plain` are the lines of data.  With each plain is a flag
/// indicating if that line should be included in the output (all lines are called, so that updates
/// can use this same code).  All methods return a result, with the Err value stopping the parse.
/// Note that the default implementations just return success, and ignore the result.
pub trait Sink {
    /// Begin an insert sequence for the given delta.
    fn insert(&mut self, _delta: usize) -> Result<()> {
        Ok(())
    }

    /// Begin a delete sequence.
    fn delete(&mut self, _delta: usize) -> Result<()> {
        Ok(())
    }

    /// End a previous insert or delete.
    fn end(&mut self, _delta: usize) -> Result<()> {
        Ok(())
    }

    /// A single line of plain text from the weave.  `keep` indicates if the line should be
    /// included in the requested delta.
    fn plain(&mut self, _text: &str, _keep: bool) -> Result<()> {
        Ok(())
    }
}

/// The PullParser returns the entries as nodes.  These are equivalent to
/// the values in Sink.
#[derive(Debug)]
pub enum Entry {
    /// Begin an insert sequence for the given delta.
    Insert { delta: usize },

    /// Begin a delete sequence.
    Delete { delta: usize },

    /// End a previous insert or delete.
    End { delta: usize },

    /// A single line of plaintext from the weave.  `keep` indicates if the
    /// line should be included in the requested delta.
    Plain { text: String, keep: bool },

    /// A control message.  Doesn't currently contain any data, which can be added later if needed.
    Control,
}

/// A Parser is used to process a weave file.  This is a wrapper around the pull parser that
/// invokes a push parser.
pub struct Parser<S: Sink, B> {
    /// The pull parser.
    pull: PullParser<B>,

    /// The sink to be given each line record in the weave file.
    sink: Rc<RefCell<S>>,

    /// A single pending line, kept from the last invocation.
    pending: Option<String>,

    /// Tracking the line number.
    lineno: usize,
}

impl<S: Sink> Parser<S, BufReader<Box<dyn Read>>> {
    /// Construct a parser, based on the main file of the naming convention.
    pub fn new(
        naming: &dyn NamingConvention,
        sink: S,
        delta: usize,
    ) -> Result<Parser<S, BufReader<Box<dyn Read>>>> {
        let rd = if naming.is_compressed() {
            let fd = File::open(naming.main_file())?;
            Box::new(GzDecoder::new(fd)) as Box<dyn Read>
        } else {
            Box::new(File::open(naming.main_file())?) as Box<dyn Read>
        };
        let lines = BufReader::new(rd).lines();
        Parser::new_raw(lines, Rc::new(RefCell::new(sink)), delta)
    }
}

impl<S: Sink, B: BufRead> Parser<S, B> {
    /// Construct a new Parser, reading from the given Reader, giving records to the given Sink,
    /// and aiming for the specified `delta`.  This is not the intended constructor, normal users
    /// should use `new`.  (This is public, for testing).
    pub fn new_raw(
        source: Lines<B>,
        sink: Rc<RefCell<S>>,
        delta: usize,
    ) -> Result<Parser<S, B>> {
        let pull = PullParser::new_raw(source, delta)?;
        Ok(Parser {
            pull,
            sink,
            pending: None,
            lineno: 0,
        })
    }

    /// Run the parser until we either reach the given line number, or the end of the weave.  Lines
    /// are numbered from 1, so calling with a lineno of zero will run the parser until the end of
    /// the input.  Returns Ok(0) for the end of input, Ok(n) for stopping at line n (which should
    /// always be the same as the passed in lineno, or Err if there is an error.
    pub fn parse_to(&mut self, lineno: usize) -> Result<usize> {
        // Handle any pending input line.  Pending lines only happen while keeping.
        if let Some(text) = mem::replace(&mut self.pending, None) {
            self.sink.borrow_mut().plain(&text, true)?;
        }

        loop {
            match self.pull.next() {
                Some(Ok(Entry::Plain { text, keep })) => {
                    if keep {
                        self.lineno += 1;
                        if self.lineno == lineno {
                            // This is the desired stopping point, hold onto this line, and return
                            // to the caller.
                            self.pending = Some(text);
                            return Ok(lineno);
                        }
                    }

                    self.sink.borrow_mut().plain(&text, keep)?;
                }
                Some(Ok(Entry::Insert { delta })) => {
                    self.sink.borrow_mut().insert(delta)?;
                }
                Some(Ok(Entry::Delete { delta })) => {
                    self.sink.borrow_mut().delete(delta)?;
                }
                Some(Ok(Entry::End { delta })) => {
                    self.sink.borrow_mut().end(delta)?;
                }
                Some(Ok(Entry::Control)) => (),
                Some(Err(err)) => {
                    return Err(err);
                }
                None => {
                    return Ok(0);
                }
            }
        }
    }


    /// Get the header read from this weave file.
    pub fn get_header(&self) -> &Header {
        &self.pull.header
    }

    /// Consume the parser, returning the header.
    pub fn into_header(self) -> Header {
        self.pull.into_header()
    }

    /// Get a copy of the sink.
    pub fn get_sink(&self) -> Rc<RefCell<S>> {
        self.sink.clone()
    }
}

/*
/// A PullIterator returns entities in a weave file, extracting either
/// everything, or only a specific delta.
pub struct PullIterator<B> {
    /// The lines of the input.
    source: Lines<B>,

    /// The desired delta to retrieve.
    delta: usize,

    /// The delta state is kept sorted with the newest (largest) delta at
    /// element 0.
    delta_state: Vec<OneDelta>,

    /// Indicates we are currently keeping lines.
    keeping: bool,

    /// The current line number.
    lineno: usize,

    /// The header extracted from the file.
    header: Header,
}
*/

pub struct PullParser<B> {
    /// The lines of the input.
    source: Lines<B>,

    /// The desired delta to retrieve.
    delta: usize,

    /// The delta state is kept sorted with the newest (largest) delta at element 0.
    delta_state: Vec<OneDelta>,

    /// Indicates that we are currently "keeping" lines.
    keeping: bool,

    /// The header extracted from the file.
    header: Header,
}

impl PullParser<BufReader<Box<dyn Read>>> {
    /// Construct a parser, based on the main file of the naming
    /// convention.
    pub fn new(
        naming: &dyn NamingConvention,
        delta: usize,
    ) -> Result<PullParser<BufReader<Box<dyn Read>>>> {
        let rd = if naming.is_compressed() {
            let fd = File::open(naming.main_file())?;
            Box::new(GzDecoder::new(fd)) as Box<dyn Read>
        } else {
            Box::new(File::open(naming.main_file())?) as Box<dyn Read>
        };
        let lines = BufReader::new(rd).lines();
        PullParser::new_raw(lines, delta)
    }
}

impl<B: BufRead> PullParser<B> {
    /// Construct a new Parser, reading from the given Reader.  The parser
    /// will act as an iterator.  This is the intended constructor, normal
    /// users should use `new`.  (This is public for testing).
    pub fn new_raw(mut source: Lines<B>, delta: usize) -> Result<PullParser<B>> {
        if let Some(line) = source.next() {
            let line = line?;
            let header = Header::decode(&line)?;

            Ok(PullParser {
                source,
                delta,
                delta_state: vec![],
                keeping: false,
                header,
            })
        } else {
            Err(Error::EmptyWeave)
        }
    }

    /// Remove the given numbered state.
    fn pop(&mut self, delta: usize) {
        // The binary search is reversed, so the largest are first.
        let pos = match self
            .delta_state
            .binary_search_by(|ent| delta.cmp(&ent.delta))
        {
            Ok(pos) => pos,
            Err(_) => unreachable!(),
        };

        self.delta_state.remove(pos);
    }

    /// Add a new state.  It will be inserted in the proper place in the array, based on the delta
    /// number.
    fn push(&mut self, delta: usize, mode: StateMode) {
        match self
            .delta_state
            .binary_search_by(|ent| delta.cmp(&ent.delta))
        {
            Ok(_) => panic!("Duplicate state in push"),
            Err(pos) => self.delta_state.insert(pos, OneDelta { delta, mode }),
        }
    }

    /// Update the keep field, based on the current state.
    fn update_keep(&mut self) {
        info!("Update: {:?}", self.delta_state);
        for st in &self.delta_state {
            match st.mode {
                StateMode::Keep => {
                    self.keeping = true;
                    return;
                }
                StateMode::Skip => {
                    self.keeping = false;
                    return;
                }
                _ => (),
            }
        }

        // This shouldn't be reached if there are any more context lines, but we may get here when
        // we reach the end of the input.
        self.keeping = false;
    }

    /// Get the header read from this weave file.
    pub fn get_header(&self) -> &Header {
        &self.header
    }

    /// Consume the parser, returning the header.
    pub fn into_header(self) -> Header {
        self.header
    }
}

impl<B: BufRead> Iterator for PullParser<B> {
    type Item = Result<Entry>;

    fn next(&mut self) -> Option<Result<Entry>> {
        // At this level, there is a 1:1 correspondence between weave input
        // lines and those returned.
        let line = match self.source.next() {
            None => return None,
            Some(Ok(line)) => line,
            Some(Err(e)) => return Some(Err(From::from(e))),
        };

        info!("line: {:?}", line);

        // Detect the first character, without borrowing.
        let textual = match line.bytes().next() {
            None => true,
            Some(ch) if ch != b'\x01' => true,
            _ => false,
        };

        if textual {
            return Some(Ok(Entry::Plain {
                text: line,
                keep: self.keeping,
            }));
        }

        let linebytes = line.as_bytes();

        if linebytes.len() < 4 {
            return Some(Ok(Entry::Control));
        }

        if linebytes[1] != b'I' && linebytes[1] != b'D' && linebytes[1] != b'E' {
            return Some(Ok(Entry::Control));
        };

        // TODO: Don't panic, but fail.
        let this_delta: usize = line[3..].parse().unwrap();

        match linebytes[1] {
            b'E' => {
                self.pop(this_delta);
                self.update_keep();
                Some(Ok(Entry::End { delta: this_delta }))
            }
            b'I' => {
                if self.delta >= this_delta {
                    self.push(this_delta, StateMode::Keep);
                } else {
                    self.push(this_delta, StateMode::Skip);
                }
                self.update_keep();

                Some(Ok(Entry::Insert { delta: this_delta }))
            }
            b'D' => {
                if self.delta >= this_delta {
                    self.push(this_delta, StateMode::Skip);
                } else {
                    self.push(this_delta, StateMode::Next);
                }
                self.update_keep();

                Some(Ok(Entry::Delete { delta: this_delta }))
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum StateMode {
    Keep,
    Skip,
    Next,
}

#[derive(Debug)]
struct OneDelta {
    delta: usize,
    mode: StateMode,
}
