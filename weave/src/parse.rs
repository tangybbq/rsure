//! Weave parsing

use Result;
use std::io::{BufRead, Lines};
use std::mem;

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

/// A Parser is used to process a weave file, extracting either everything, or only a specific
/// delta.
pub struct Parser<'s, S: 's + Sink, B> {
    /// The lines of the input.
    source: Lines<B>,

    /// The sink to be given each line record in the weave file.
    sink: &'s mut S,

    /// The desired delta to retrieve, which affects the parse_to call as well as the `keep`
    /// argument passed to the sink's `plain` call.
    delta: usize,

    /// The delta state is kept sorted with the newest (largest) delta at element 0.
    delta_state: Vec<OneDelta>,

    /// A pending input line kept from the last invocation.
    pending: Option<String>,

    /// Indicates that we are currently "keeping" lines.
    keeping: bool,

    /// The line number indicator.
    lineno: usize,
}

impl<'s, S: 's + Sink, B: BufRead> Parser<'s, S, B> {
    /// Construct a new Parser, reading from the given Reader, giving records to the given Sink,
    /// and aiming for the specified `delta`.
    pub fn new<'ss>(source: Lines<B>, sink: &'ss mut S, delta: usize) -> Parser<'ss, S, B> {
        Parser {
            source: source,
            sink: sink,
            delta: delta,
            delta_state: vec![],
            pending: None,
            keeping: false,
            lineno: 0,
        }
    }

    /// Run the parser until we either reach the given line number, or the end of the weave.  Lines
    /// are numbered from 1, so calling with a lineno of zero will run the parser until the end of
    /// the input.  Returns Ok(0) for the end of input, Ok(n) for stopping at line n (which should
    /// always be the same as the passed in lineno, or Err if there is an error.
    pub fn parse_to(&mut self, lineno: usize) -> Result<usize> {
        // Handle any pending input line.
        if let Some(pending) = mem::replace(&mut self.pending, None) {
            self.sink.plain(&pending, self.keeping)?;
        }

        loop {
            // Get the next input line, finishing if we are done.
            let line = match self.source.next() {
                None => return Ok(0),
                Some(line) => line?,
            };

            info!("line: {:?}", line);

            // Detect the first character, without borrowing.
            let textual = match line.bytes().next() {
                None => true,
                Some(ch) if ch != b'\x01' => true,
                _ => false,
            };

            if textual {
                // Textual line.  Count line numbers for the lines we're keeping.
                if self.keeping {
                    self.lineno += 1;
                    if self.lineno == lineno {
                        // This is the desired stopping point, hold onto this line, and return to
                        // the caller.
                        self.pending = Some(line);
                        return Ok(lineno);
                    }
                }

                // Otherwise, call the Sink, and continue.
                info!("textual: keeping={}", self.keeping);
                self.sink.plain(&line, self.keeping)?;
                continue;
            }

            let linebytes = line.as_bytes();

            // At this point, all should be control lines.  Skip any that are too short.
            if linebytes.len() < 4 {
                continue;
            }

            // Ignore control lines other than the insert/delete/end lines.
            if linebytes[1] != b'I' && linebytes[1] != b'D' && linebytes[1] != b'E' {
                continue;
            }

            let this_delta: usize = line[3..].parse()?;

            match linebytes[1] {
                b'E' => {
                    self.sink.end(this_delta)?;
                    self.pop(this_delta);
                }
                b'I' => {
                    self.sink.insert(this_delta)?;

                    // Do this insert if this insert is at least as old as the requested delta.
                    if self.delta >= this_delta {
                        self.push(this_delta, StateMode::Keep);
                    } else {
                        self.push(this_delta, StateMode::Skip);
                    }
                }
                b'D' => {
                    self.sink.delete(this_delta)?;

                    // Do this delete if this delete is newer than current.  If not, don't account
                    // for it.
                    if self.delta >= this_delta {
                        self.push(this_delta, StateMode::Skip);
                    } else {
                        self.push(this_delta, StateMode::Next);
                    }
                }
                _ => unreachable!(),
            }

            self.update_keep();
        }
    }

    /// Remove the given numbered state.
    fn pop(&mut self, delta: usize) {
        // The binary search is reversed, so the largest are first.
        let pos = match self.delta_state.binary_search_by(|ent| delta.cmp(&ent.delta)) {
            Ok(pos) => pos,
            Err(_) => panic!("State of pop not present"),
        };

        self.delta_state.remove(pos);
    }

    /// Add a new state.  It will be inserted in the proper place in the array, based on the delta
    /// number.
    fn push(&mut self, delta: usize, mode: StateMode) {
        match self.delta_state.binary_search_by(|ent| delta.cmp(&ent.delta)) {
            Ok(_) => panic!("Duplicate state in push"),
            Err(pos) => self.delta_state.insert(pos, OneDelta {
                delta: delta,
                mode: mode,
            }),
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
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum StateMode { Keep, Skip, Next }

#[derive(Debug)]
struct OneDelta {
    delta: usize,
    mode: StateMode,
}
