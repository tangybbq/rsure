//! Use bitkeeper to manage multiple versions of surefiles.
//!
//! Now that BitKeeper is [open source](http://bitkeeper.org/), let's make
//! it available as a store for surefiles.  Some brief experimenting shows
//! that the 'weave' method BitKeeper uses to store multiple file revisions
//! works rather well for surefiles.  One test case, for example, was able
//! to take nearly 2GB of individually compressed surefiles (several
//! hundred), and encode them in less than 50MB.

use errors::ErrorKind;
use regex::Regex;
use Result;
use std::fs::File;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use suretree::SureTree;

/// Initialize a new BitKeeper-based storage directory.  The path should
/// name a directory that is either empty or can be created with a single
/// `mkdir`.
pub fn setup<P: AsRef<Path>>(base: P) -> Result<()> {
    let base = base.as_ref();

    let mut cmd = Command::new("bk");
    // BAM=off is needed to keep BK from storing large files as just whole
    // files.  Surefiles will often be large, and the delta storage is the
    // whole reason we're using BK.
    // checkout=none frees up some space by not leaving uncompressed copies
    // of the surefiles in the work directory.
    cmd.args(&["setup", "-f", "-FBAM=off", "-Fcheckout=none"]);
    cmd.arg(base.as_os_str());
    let status = try!(cmd.status());
    if !status.success() {
        return Err(ErrorKind::BkError(status, "".into()).into());
    }

    // Construct a README file in this directory, since there won't appear
    // to be files in it, other than the BitKeeper directory.
    {
        let mut ofd = try!(File::create(base.join("README")));
        try!(ofd.write_all(include_bytes!("../etc/template-bk-readme.txt")));
    }

    let status = try!(Command::new("bk")
                      .args(&["ci", "-iu", "README"])
                      .current_dir(base)
                      .status());
    if !status.success() {
        return Err(ErrorKind::BkError(status, "".into()).into());
    }

    let status = try!(Command::new("bk")
                      .args(&["commit", "-yInitial README"])
                      .current_dir(base)
                      .status());
    if !status.success() {
        return Err(ErrorKind::BkError(status, "".into()).into());
    }

    Ok(())
}

/// A manager for a tree of surefiles managed under BitKeeper.
pub struct BkDir {
    base: PathBuf,
    change_re: Regex,
}

impl BkDir {
    /// Construct a new `BkDir` that can store and retrieve files from the
    /// given path.  The directory must have already been created with the
    /// `setup` function above.
    pub fn new<P: AsRef<Path>>(base: P) -> Result<BkDir> {
        Ok(BkDir {
            base: base.as_ref().to_owned(),
            change_re: Regex::new(r"^  ([^ ]+) ([\d\.]+) (.*)$").unwrap(),
        })
    }

    /// Write a SureTree to a surefile of the given name (generally of the
    /// convention "fsname.dat").  If this is the first file written by
    /// this name, it will be checked in initially into BitKeeper.  If it
    /// is an update, the BitKeeper file will be edited, and a cset made to
    /// record this.  The `info` text should be a short piece of text to
    /// describe this revision.  It will be used as the commit text within
    /// BitKeeper, and also used (exactly) to retrieve this version later.
    pub fn save(&self, tree: &SureTree, name: &str, info: &str) -> Result<()> {
        let y_arg = format!("-y{}", info);

        // Try checking out the file.  BitKeeper will fail if it doesn't
        // exist.
        let initial = match self.bk_do(&["edit", name]) {
            Ok(_) => false,
            Err(_) => true,
        };

        {
            let mut wr = try!(File::create(&self.base.join(name)));
            try!(tree.save_to(&mut wr));
        }

        if initial {
            try!(self.bk_do(&["ci", "-i", &y_arg, name]));
        } else {
            try!(self.bk_do(&["ci", "-f", &y_arg, name]));
        }

        try!(self.bk_do(&["commit", &y_arg, name]));

        Ok(())
    }

    /// Query to determine all file versions that have been saved.
    pub fn query(&self) -> Result<Vec<BkSureFile>> {
        let output = try!(Command::new("bk")
                          .args(&["changes", "-v",
                                "-d:INDENT::DPN: :REV: :C:\n"])
                          .current_dir(&self.base)
                          .output());
        if !output.stderr.is_empty() {
            return Err(ErrorKind::BkError(output.status,
                                          String::from_utf8_lossy(&output.stderr).into_owned()).into());
        }
        if !output.status.success() {
            return Err(ErrorKind::BkError(output.status, "".into()).into());
        }

        let mut result = vec![];

        for line in (&output.stdout[..]).lines() {
            let line = try!(line);
            match self.change_re.captures(&line) {
                None => (),
                Some(cap) => {
                    let file = cap.at(1).unwrap();
                    let rev = cap.at(2).unwrap();
                    let name = cap.at(3).unwrap();
                    if !file.ends_with(".dat") {
                        continue;
                    }
                    result.push(BkSureFile {
                        file: file.to_owned(),
                        rev: rev.to_owned(),
                        name: name.to_owned(),
                    });
                },
            }
        }
        Ok(result)
    }

    pub fn load(&self, file: &str, name: &str) -> Result<SureTree> {
        let files = try!(self.query());
        let rev = match files.iter().find(|&x| x.file == file && x.name == name) {
            None => return Err(format!("Couldn't find file: {:?} name: {:?}", file, name).into()),
            Some(x) => &x.rev[..],
        };

        let mut child = try!(Command::new("bk")
                             .args(&["co", "-p",
                                   &format!("-r{}", rev),
                                   file])
                             .current_dir(&self.base)
                             .stdout(Stdio::piped())
                             .spawn());
        let tree = try!(SureTree::load_from(child.stdout.as_mut().unwrap()));
        let status = try!(child.wait());
        if !status.success() {
            return Err(ErrorKind::BkError(status, "".into()).into());
        }
        Ok(tree)
    }

    fn bk_do(&self, args: &[&str]) -> Result<()> {
        let status = try!(Command::new("bk")
                          .args(args)
                          .current_dir(&self.base)
                          .status());
        if !status.success() {
            return Err(ErrorKind::BkError(status, "".into()).into());
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct BkSureFile {
    pub file: String,
    pub rev: String,
    pub name: String,
}
