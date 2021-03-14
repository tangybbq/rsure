/// The sure stream.
///
/// The sure stream represents a linearization of a SureTree.  By keeping
/// representations as iterators across SureNodes instead of keeping an
/// entire tree in memory, we can process larger filesystem trees, using
/// temporary space on the hard disk instead of using memory.
use crate::{suretree::AttMap, Error, Result};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use std::{
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};
use weave::NamingConvention;

mod compare;
pub mod fs;
mod fullpath;
mod hashes;

pub use compare::compare_trees;
pub use fullpath::into_tracker;
pub use hashes::{HashCombiner, HashUpdater, Source};

#[derive(Clone, Debug)]
pub enum SureNode {
    Enter { name: String, atts: AttMap },
    Leave,
    File { name: String, atts: AttMap },
    Sep,
}

impl SureNode {
    pub fn is_enter(&self) -> bool {
        matches!(self, SureNode::Enter { .. })
    }

    pub fn is_reg_file(&self) -> bool {
        match self {
            SureNode::File { atts, .. } => atts["kind"] == "file",
            _ => false,
        }
    }

    pub fn is_file(&self) -> bool {
        matches!(self, SureNode::File { .. })
    }

    pub fn is_leave(&self) -> bool {
        matches!(self, SureNode::Leave)
    }

    pub fn is_sep(&self) -> bool {
        matches!(self, SureNode::Sep)
    }

    pub fn needs_hash(&self) -> bool {
        match self {
            SureNode::File { atts, .. } => atts["kind"] == "file" && !atts.contains_key("sha1"),
            _ => false,
        }
    }

    pub fn size(&self) -> u64 {
        match self {
            SureNode::File { atts, .. } => {
                atts.get("size").map(|x| x.parse().unwrap()).unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// Get the name of this node.  Panics if the node type does not have
    /// an associated name.
    pub fn name(&self) -> &str {
        match self {
            SureNode::File { ref name, .. } => name,
            SureNode::Enter { ref name, .. } => name,
            _ => panic!("Node does not have a name"),
        }
    }

    /// Safely get the name of this node.
    pub fn get_name(&self) -> Option<&str> {
        match self {
            SureNode::File { ref name, .. } => Some(name),
            SureNode::Enter { ref name, .. } => Some(name),
            _ => None,
        }
    }

    /// Get a nice representation of the kind of this node.  Returns "???"
    /// if the kind isn't meaningful.
    pub fn kind(&self) -> &str {
        self.atts()
            .map(|a| a.get("kind").map(|k| &k[..]).unwrap_or("???"))
            .unwrap_or("???")
    }

    /// Access the nodes attributes.
    pub fn atts(&self) -> Option<&AttMap> {
        match self {
            SureNode::File { ref atts, .. } => Some(atts),
            SureNode::Enter { ref atts, .. } => Some(atts),
            _ => None,
        }
    }

    /// Access the nodes attributes mutably.
    pub fn atts_mut(&mut self) -> Option<&mut AttMap> {
        match self {
            SureNode::File { ref mut atts, .. } => Some(atts),
            SureNode::Enter { ref mut atts, .. } => Some(atts),
            _ => None,
        }
    }
}

// TODO: These might be possible to make more generic, but it gets messy,
// as it might just be best to assume failure.

/// Write a sure iterator to a standard gzipped file of the given name.
pub fn save<P, I>(name: P, nodes: I) -> Result<()>
where
    P: AsRef<Path>,
    I: Iterator<Item = Result<SureNode>>,
{
    let wr = File::create(name)?;
    let wr = GzEncoder::new(wr, Compression::default());
    save_to(wr, nodes)
}

/// Write a sure iterator to a new temp file with a given naming
/// convention.  Returns the name of the file, if it could be created.  The
/// data will not be written compressed.
pub fn save_naming<I, N>(naming: &N, nodes: I) -> Result<PathBuf>
where
    N: NamingConvention,
    I: Iterator<Item = Result<SureNode>>,
{
    let (tmp_name, mut tmp_file) = naming.temp_file()?;
    save_to(&mut tmp_file, nodes)?;
    Ok(tmp_name)
}

/// Save a sure tree to the given writer.
pub fn save_to<W, I>(wr: W, nodes: I) -> Result<()>
where
    W: Write,
    I: Iterator<Item = Result<SureNode>>,
{
    let mut wr = BufWriter::new(wr);

    writeln!(&mut wr, "asure-2.0")?;
    writeln!(&mut wr, "-----")?;

    for node in nodes {
        match node? {
            SureNode::Enter { name, atts } => header(&mut wr, 'd', &name, &atts)?,
            SureNode::File { name, atts } => header(&mut wr, 'f', &name, &atts)?,
            SureNode::Sep => writeln!(&mut wr, "-")?,
            SureNode::Leave => writeln!(&mut wr, "u")?,
        }
    }
    Ok(())
}

/// For pushed based writing, we can also write using a NodeWriter.
pub struct NodeWriter<W: Write> {
    writer: BufWriter<W>,
}

impl<W: Write> NodeWriter<W> {
    pub fn new(writer: W) -> Result<NodeWriter<W>> {
        let mut wr = BufWriter::new(writer);
        writeln!(&mut wr, "asure-2.0")?;
        writeln!(&mut wr, "-----")?;

        Ok(NodeWriter { writer: wr })
    }

    pub fn write_node(&mut self, node: &SureNode) -> Result<()> {
        match node {
            SureNode::Enter { name, atts } => header(&mut self.writer, 'd', &name, &atts)?,
            SureNode::File { name, atts } => header(&mut self.writer, 'f', &name, &atts)?,
            SureNode::Sep => writeln!(&mut self.writer, "-")?,
            SureNode::Leave => writeln!(&mut self.writer, "u")?,
        }
        Ok(())
    }
}

fn header<W: Write>(out: &mut W, kind: char, name: &str, atts: &AttMap) -> Result<()> {
    write!(out, "{}{} [", kind, name)?;

    for (k, v) in atts {
        write!(out, "{} {} ", k, v)?;
    }
    writeln!(out, "]")?;
    Ok(())
}

/// Load and iterate a sure tree from a standard gzip compressed surefile.
pub fn load<P: AsRef<Path>>(name: P) -> Result<ReadIterator<GzDecoder<File>>> {
    let rd = File::open(name)?;
    let rd = GzDecoder::new(rd);
    load_from(rd)
}

/// Load a surenode sequence from the given reader.
pub fn load_from<R: Read>(rd: R) -> Result<ReadIterator<R>> {
    let rd = BufReader::new(rd);
    let mut lines = rd.split(b'\n');

    fixed(&mut lines, b"asure-2.0")?;
    fixed(&mut lines, b"-----")?;

    Ok(ReadIterator {
        lines: lines,
        depth: 0,
        done: false,
    })
}

fn fixed<I>(inp: &mut I, exp: &[u8]) -> Result<()>
where
    I: Iterator<Item = io::Result<Vec<u8>>>,
{
    match inp.next() {
        Some(Ok(ref text)) if &text[..] == exp => Ok(()),
        Some(Ok(ref text)) => Err(Error::UnexpectedLine(
            String::from_utf8_lossy(text).into_owned(),
            String::from_utf8_lossy(exp).into_owned(),
        )),
        Some(Err(e)) => Err(Error::SureFileError(e)),
        None => Err(Error::SureFileEof),
    }
}

pub struct ReadIterator<R> {
    lines: io::Split<BufReader<R>>,
    depth: usize,
    done: bool,
}

impl<R: Read> Iterator for ReadIterator<R> {
    type Item = Result<SureNode>;

    fn next(&mut self) -> Option<Result<SureNode>> {
        if self.done {
            return None;
        }

        let line = match self.get_line() {
            Ok(line) => line,
            Err(e) => return Some(Err(e)),
        };

        match line[0] {
            b'd' => {
                let (dname, datts) = decode_entity(&line[1..]);
                self.depth += 1;
                Some(Ok(SureNode::Enter {
                    name: dname,
                    atts: datts,
                }))
            }
            b'f' => {
                let (fname, fatts) = decode_entity(&line[1..]);
                Some(Ok(SureNode::File {
                    name: fname,
                    atts: fatts,
                }))
            }
            b'-' => Some(Ok(SureNode::Sep)),
            b'u' => {
                self.depth -= 1;
                if self.depth == 0 {
                    self.done = true;
                }
                Some(Ok(SureNode::Leave))
            }
            ch => Some(Err(Error::InvalidSurefileChar(ch as char))),
        }
    }
}

impl<R: Read> ReadIterator<R> {
    fn get_line(&mut self) -> Result<Vec<u8>> {
        match self.lines.next() {
            None => return Err(Error::TruncatedSurefile),
            Some(l) => Ok(l?),
        }
    }
}

// TODO: This should return Result to handle errors.
pub(crate) fn decode_entity(text: &[u8]) -> (String, AttMap) {
    let (name, mut text) = get_delim(text, b' ');
    assert!(text[0] == b'[');
    text = &text[1..];

    let mut atts = AttMap::new();
    while text[0] != b']' {
        let (key, t2) = get_delim(text, b' ');
        let (value, t2) = get_delim(t2, b' ');
        text = t2;

        atts.insert(key, value);
    }

    (name, atts)
}

fn get_delim(text: &[u8], delim: u8) -> (String, &[u8]) {
    let mut it = text.iter();
    let space = it.position(|&s| s == delim).unwrap();
    (
        String::from_utf8(text[..space].to_owned()).unwrap(),
        &text[space + 1..],
    )
}
