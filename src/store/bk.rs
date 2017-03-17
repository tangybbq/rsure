/// Storage manager that uses BitKeeper.

use ::Result;
use ::SureTree;
use errors::ErrorKind;

use regex::Regex;
use std::fs::File;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use super::{Store, StoreTags, Version};

pub struct BkStore {
    pub base: PathBuf,
    pub name: String,
    pub change_re: Regex,
}

impl BkStore {
    pub fn new(base: &Path, name: &str) -> BkStore {
        BkStore {
            base: base.to_path_buf(),
            name: format!("{}.dat", name),
            change_re: Regex::new(r"^  ([^ ]+) ([\d\.]+) (.*)$").unwrap(),
        }
    }
}

impl Store for BkStore {
    fn write_new(&self, tree: &SureTree, tags: &StoreTags) -> Result<()> {
        let tag_name = name_of_tags(tags);
        let y_arg = format!("-y{}", tag_name);

        // Try checking out the file.  BitKeeper will fail if it doesn't
        // exist.
        let initial = match self.bk_do(&["edit", &self.name]) {
            Ok(_) => false,
            Err(_) => true,
        };

        {
            let mut wr = File::create(&self.base.join(&self.name))?;
            tree.save_to(&mut wr)?;
        }

        if initial {
            self.bk_do(&["ci", "-i", &y_arg, &self.name])?;
        } else {
            self.bk_do(&["ci", "-f", &y_arg, &self.name])?;
        }

        self.bk_do(&["commit", &y_arg, &self.name])?;

        Ok(())
    }

    fn load(&self, version: Version) -> Result<SureTree> {
        let vers = self.get_version(version)?;
        let rev = match vers {
            None => return Err("Couldn't find bk version".into()),
            Some(ref x) => &x.rev[..],
        };

        let mut child = Command::new("bk")
            .args(&["co", "-p",
                  &format!("-r{}", rev),
                  &self.name])
            .current_dir(&self.base)
            .stdout(Stdio::piped())
            .spawn()?;
        let tree = SureTree::load_from(child.stdout.as_mut().unwrap())?;
        let status = child.wait()?;
        if !status.success() {
            return Err(ErrorKind::BkError(status, "".into()).into());
        }
        Ok(tree)
    }
}

impl BkStore {
    fn bk_do(&self, args: &[&str]) -> Result<()> {
        let status = Command::new("bk")
            .args(args)
            .current_dir(&self.base)
            .status()?;
        if !status.success() {
            return Err(ErrorKind::BkError(status, "".into()).into());
        }
        Ok(())
    }

    /// Map a version to version information.
    fn get_version(&self, version: Version) -> Result<Option<BkSureFile>> {
        let versions = self.query()?;
        let mut versions = versions.into_iter().filter(|x| x.file == self.name);
        let index = match version {
            Version::Latest => 0,
            Version::Prior => 1,
        };
        Ok(versions.nth(index))
    }

    /// Query to determine all file versions that have been saved.
    pub fn query(&self) -> Result<Vec<BkSureFile>> {
        let output = Command::new("bk")
            .args(&["changes", "-v",
                  "-d:INDENT::DPN: :REV: :C:\n"])
            .current_dir(&self.base)
            .output()?;
        if !output.stderr.is_empty() {
            return Err(ErrorKind::BkError(output.status,
                                          String::from_utf8_lossy(&output.stderr).into_owned()).into());
        }
        if !output.status.success() {
            return Err(ErrorKind::BkError(output.status, "".into()).into());
        }

        let mut result = vec![];

        for line in (&output.stdout[..]).lines() {
            let line = line?;
            match self.change_re.captures(&line) {
                None => (),
                Some(cap) => {
                    let file = cap.get(1).unwrap().as_str();
                    let rev = cap.get(2).unwrap().as_str();
                    let name = cap.get(3).unwrap().as_str();
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
}

#[derive(Debug)]
pub struct BkSureFile {
    pub file: String,
    pub rev: String,
    pub name: String,
}

fn name_of_tags(tags: &StoreTags) -> String {
    if tags.len() != 1 {
        panic!("Must be a single tag name=...");
    }

    match tags.get("name") {
        None => panic!("Must be a single tag name=..."),
        Some(x) => x.clone(),
    }
}

/// Initialize a new BitKeeper-based storage directory.  The path should
/// name a directory that is either empty or can be created with a single
/// `mkdir`.
pub fn bk_setup<P: AsRef<Path>>(base: P) -> Result<()> {
    let base = base.as_ref();

    let mut cmd = Command::new("bk");
    // BAM=off is needed to keep BK from storing large files as just whole
    // files.  Surefiles will often be large, and the delta storage is the
    // whole reason we're using BK.
    // checkout=none frees up some space by not leaving uncompressed copies
    // of the surefiles in the work directory.
    cmd.args(&["setup", "-f", "-FBAM=off", "-Fcheckout=none"]);
    cmd.arg(base.as_os_str());
    let status = cmd.status()?;
    if !status.success() {
        return Err(ErrorKind::BkError(status, "".into()).into());
    }

    // Construct a README file in this directory, since there won't appear
    // to be files in it, other than the BitKeeper directory.
    {
        let mut ofd = File::create(base.join("README"))?;
        ofd.write_all(include_bytes!("../../etc/template-bk-readme.txt"))?;
    }

    let status = Command::new("bk")
        .args(&["ci", "-iu", "README"])
        .current_dir(base)
        .status()?;
    if !status.success() {
        return Err(ErrorKind::BkError(status, "".into()).into());
    }

    let status = Command::new("bk")
        .args(&["commit", "-yInitial README"])
        .current_dir(base)
        .status()?;
    if !status.success() {
        return Err(ErrorKind::BkError(status, "".into()).into());
    }

    Ok(())
}
