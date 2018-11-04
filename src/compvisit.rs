//! Comparison visitors

use std::{
    io::{self, Write},
    path::Path,
};

pub trait CompareVisitor {
    /// Visit a node that has been changed.  The name is the path, relative to the start of the
    /// tree.  The action indicates what happened to the file.  The attributes describe what
    /// happened to the file attributes (e.g., attributes modified).
    fn visit(
        &mut self,
        name: &Path,
        kind: CompareType,
        action: CompareAction,
        atts: Option<&[String]>,
    );
}

pub enum CompareType {
    Dir,
    NonDir,
}

pub enum CompareAction {
    Add,
    Delete,
    Modify,
}

/// A visitor that just prints out the changes, textually, to a given writer.
pub struct PrintVisitor<W: Write>(W);

// Note that these can't be methods, since that would require an unnecessary type to know which
// instance of the struct.
/// A `PrintVisitor` that prints the names to the standard output.
pub fn stdout_visitor() -> PrintVisitor<io::Stdout> {
    PrintVisitor(io::stdout())
}

/// A `PrintVisitor` that prints the names to the standard error.
pub fn stderr_visitor() -> PrintVisitor<io::Stderr> {
    PrintVisitor(io::stderr())
}

impl<W: Write> CompareVisitor for PrintVisitor<W> {
    fn visit(
        &mut self,
        name: &Path,
        kind: CompareType,
        action: CompareAction,
        atts: Option<&[String]>,
    ) {
        let act = match action {
            CompareAction::Add => '+',
            CompareAction::Delete => '-',
            CompareAction::Modify => ' ',
        };

        let kind = match kind {
            CompareType::Dir => "dir",
            CompareType::NonDir => "file",
        };

        let atts = match atts {
            None => format!("{:22}", kind),
            Some(atts) => {
                let mut message = vec![];
                for ent in atts {
                    write!(&mut message, ",{}", ent).unwrap();
                }
                let message = String::from_utf8(message).unwrap();
                format!("[{:<20}]", &message[1..])
            }
        };

        writeln!(self.0, "{} {} {}", act, atts, name.to_string_lossy()).unwrap();
    }
}
