// Playing with paths.

extern crate chrono;
extern crate rsure;
extern crate env_logger;
extern crate regex;

#[macro_use]
extern crate log;

#[macro_use]
extern crate clap;

use chrono::Local;
use clap::{App, AppSettings, Arg, SubCommand};

use std::collections::BTreeMap;
use std::path::Path;

use rsure::{show_tree, Progress, SureHash, TreeCompare, stdout_visitor, parse_store, StoreTags,
            StoreVersion, Version};

mod bkcmd;

// For now, just use the crate's error type.
pub use rsure::Result;

#[allow(dead_code)]
fn main() {
    env_logger::init().unwrap();

    let matches = App::new("rsure")
        .version(crate_version!())
        .setting(AppSettings::GlobalVersion)
        .arg(
            Arg::with_name("file")
                .short("f")
                .long("file")
                .takes_value(true)
                .help(
                    "Base of file name, default 2sure, will get .dat.gz appended",
                ),
        )
        .arg(
            Arg::with_name("dir")
                .short("d")
                .long("dir")
                .takes_value(true)
                .help("Directory to scan, defaults to \".\""),
        )
        .arg(
            Arg::with_name("tag")
                .long("tag")
                .takes_value(true)
                .multiple(true)
                .help("key=value to associate with scan"),
        )
        .arg(
            Arg::with_name("version")
                .short("v")
                .long("version")
                .takes_value(true)
                .help("Version of sure data to use (see 'list' output)"),
        )
        .setting(AppSettings::SubcommandRequired)
        .subcommand(SubCommand::with_name("scan").about(
            "Scan a directory for the first time",
        ))
        .subcommand(SubCommand::with_name("update").about(
            "Update the scan using the dat file",
        ))
        .subcommand(SubCommand::with_name("check").about(
            "Compare the directory with the dat file",
        ))
        .subcommand(SubCommand::with_name("signoff").about(
            "Compare the dat file with the bak file",
        ))
        .subcommand(SubCommand::with_name("show").about(
            "Pretty print the dat file",
        ))
        .subcommand(
            SubCommand::with_name("bknew")
                .about("Create a new bitkeeper-based sure store")
                .arg(Arg::with_name("dir").required(true).help(
                    "Directory to create bk-based store",
                )),
        )
        .subcommand(
            SubCommand::with_name("bkimport")
                .about("Import a tree of surefiles into a bk store")
                .arg(
                    Arg::with_name("src")
                        .long("src")
                        .takes_value(true)
                        .required(true),
                )
                .arg(
                    Arg::with_name("dest")
                        .long("dest")
                        .takes_value(true)
                        .required(true),
                ),
        )
        .subcommand(SubCommand::with_name("list").about(
            "List revisions in a given sure store",
        ))
        .get_matches();

    let dir = matches.value_of("dir").unwrap_or(".");

    let file = matches.value_of("file").unwrap_or("2sure.weave.gz");
    let store = parse_store(file).unwrap();

    let mut tags = decode_tags(matches.values_of("tag"));

    add_name_tag(&mut tags, &dir);

    // Note that only the "check" command uses the version tag.
    let latest = match matches.value_of("version") {
        None => Version::Latest,
        Some(x) => Version::Tagged(x.to_string()),
    };

    match matches.subcommand() {
        ("scan", Some(_)) => {
            rsure::update(&dir, &*store, false, &tags).unwrap();
        }
        ("update", Some(_)) => {
            rsure::update(&dir, &*store, true, &tags).unwrap();
        }
        ("check", Some(_)) => {
            let old_tree = store.load(latest).unwrap();
            let mut new_tree = rsure::scan_fs(&dir).unwrap();
            let estimate = new_tree.hash_estimate();
            let pdir = &Path::new(dir);
            let mut progress = Progress::new(estimate.files, estimate.bytes);
            new_tree.hash_update(pdir, &mut progress);
            progress.flush();
            info!("check {:?}", file);
            new_tree.compare_from(&mut stdout_visitor(), &old_tree, pdir);
        }
        ("signoff", Some(_)) => {
            let old_tree = store.load(Version::Prior).unwrap();
            let new_tree = store.load(Version::Latest).unwrap();
            println!("signoff {}", file);
            new_tree.compare_from(&mut stdout_visitor(), &old_tree, &Path::new(dir));
        }
        ("show", Some(_)) => {
            println!("show {}", file);
            show_tree(&Path::new(&file)).unwrap();
        }
        ("bknew", Some(sub)) => {
            let bkdir = sub.value_of("dir").unwrap();
            bkcmd::new(bkdir).unwrap();
        }
        ("bkimport", Some(sub)) => {
            let src = sub.value_of("src").unwrap();
            let dest = sub.value_of("dest").unwrap();
            bkcmd::import(src, dest).unwrap();
        }
        ("list", Some(_)) => {
            let version = store.get_versions().unwrap();
            dump_versions(&version);
        }
        _ => {
            panic!("Unsupported command.");
        }
    }
}

/// Decode the command-line tags.  Tags should be of the form key=value, and multiple can be
/// specified, terminated by the command.  It is also possible to specify --tag multiple times.
fn decode_tags<'a, I>(tags: Option<I>) -> StoreTags
where
    I: Iterator<Item = &'a str>,
{
    match tags {
        None => BTreeMap::new(),
        Some(tags) => tags.map(|x| decode_tag(x)).collect(),
    }
}

fn decode_tag<'a>(tag: &'a str) -> (String, String) {
    let fields: Vec<_> = tag.splitn(2, '=').collect();
    if fields.len() != 2 {
        panic!("Tag must be key=value");
    }
    (fields[0].to_string(), fields[1].to_string())
}

/// If the caller doesn't specify a 'name=' tag, generate one based on the current timestamp.
/// Also will add a 'dir' attribute for where the tree was captured.
fn add_name_tag<P: AsRef<Path>>(tags: &mut StoreTags, dir: P) {
    if !tags.contains_key("name") {
        tags.insert("name".to_string(), Local::now().to_rfc3339());
    }

    if !tags.contains_key("dir") {
        tags.insert(
            "dir".to_string(),
            dir.as_ref()
                .canonicalize()
                .unwrap_or_else(|_| Path::new("invalid").to_owned())
                .to_string_lossy()
                .into_owned(),
        );
    }
}

fn dump_versions(versions: &[StoreVersion]) {
    println!("vers | Time captured       | name");
    println!("-----+---------------------+------------------");
    for v in versions {
        let vers = match v.version {
            Version::Latest => "tip",
            Version::Prior => "prev",
            Version::Tagged(ref v) => v,
        };
        println!(
            "{:4} | {} | {}",
            vers,
            v.time.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S"),
            v.name
        );
    }
}
