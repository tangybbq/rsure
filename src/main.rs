// Playing with paths.

extern crate rsure;
extern crate env_logger;
extern crate regex;

#[macro_use]
extern crate log;

#[macro_use]
extern crate clap;

use clap::{App, Arg, SubCommand};

use regex::Regex;
use std::error;
use std::fs::rename;
use std::result;
use std::path::Path;

use rsure::{scan_fs, show_tree, SureHash, SureTree, TreeCompare, TreeUpdate};

pub type Result<T> = result::Result<T, Box<error::Error + Send + Sync>>;

#[allow(dead_code)]
fn main() {
    env_logger::init().unwrap();

    let matches = App::new("rsure")
        .version(&crate_version!())
        .global_version(true)
        .arg(Arg::with_name("file")
             .short("f")
             .long("file")
             .takes_value(true)
             .help("Base of file name, default 2sure, will get .dat.gz appended"))
        .arg(Arg::with_name("src")
             .short("s")
             .long("src")
             .takes_value(true)
             .help("Source .dat file for update"))
        .arg(Arg::with_name("dir")
             .short("d")
             .long("dir")
             .takes_value(true)
             .help("Directory to scan, defaults to \".\""))
        .subcommand_required(true)
        .subcommand(SubCommand::with_name("scan")
                    .about("Scan a directory for the first time"))
        .subcommand(SubCommand::with_name("update")
                    .about("Update the scan using the dat file"))
        .subcommand(SubCommand::with_name("check")
                    .about("Compare the directory with the dat file"))
        .subcommand(SubCommand::with_name("signoff")
                    .about("Compare the dat file with the bak file"))
        .subcommand(SubCommand::with_name("show")
                    .about("Pretty print the dat file"))
        .get_matches();

    let dir = matches.value_of("dir").unwrap_or(".");

    let file = augment_suffix(matches.value_of("file").unwrap_or("2sure"), ".dat.gz");
    let src = matches.value_of("src").map(|s| augment_suffix(s, ".bak.gz"));
    let src = src.unwrap_or_else(|| replace_suffix(&file, ".bak.gz"));

    let tmp = replace_suffix(&file, ".tmp.gz");

    match matches.subcommand() {
        ("scan", Some(_)) => {
            let path = Path::new(dir);
            println!("Scan {:?}", path);
            let mut tree = scan_fs(&path).unwrap();
            println!("scan: {} {} nodes", file, tree.count_nodes());
            println!("hash: {:?}", tree.hash_estimate());
            tree.hash_update(&path);
            tree.save(&tmp).unwrap();

            // Rotate the names.
            rename(&file, &src).unwrap_or(());
            rename(&tmp, &file).unwrap_or_else(|e| {
                error!("Unable to move surefile: {:?} to {:?} ({})", &tmp, &file, e);
            });
        },
        ("update", Some(_)) => {
            let path = Path::new(dir);
            println!("Load old surefile");
            let old_tree = SureTree::load(&file).unwrap();
            println!("Scan {:?}", path);
            let mut new_tree = scan_fs(&path).unwrap();
            new_tree.update_from(&old_tree);
            // Compare
            println!("hash: {:?}", new_tree.hash_estimate());
            new_tree.hash_update(&path);
            new_tree.save(&tmp).unwrap();

            // Rotate the names.
            rename(&file, &src).unwrap_or(());
            rename(&tmp, &file).unwrap_or_else(|e| {
                error!("Unable to move surefile: {:?} to {:?} ({})", &tmp, &file, e);
            });
        },
        ("check", Some(_)) => {
            println!("check {}", file);
        },
        ("signoff", Some(_)) => {
            let old_tree = SureTree::load(&src).unwrap();
            let new_tree = SureTree::load(&file).unwrap();
            println!("signoff {:?} -> {}", src, file);
            new_tree.compare_from(&old_tree, &Path::new(dir));
        },
        ("show", Some(_)) => {
            println!("show {}", file);
            show_tree(&Path::new(&file)).unwrap();
        },
        _ => {
            panic!("Unsupported command.");
        }
    }
}

// Augment the name with the given extension (e.g. ".dat.gz").  If the name
// already has some kind of extension, leave it alone, though.
fn augment_suffix(name: &str, ext: &str) -> String {
    let pat = Regex::new(r"\.(dat|bak)\.gz$").unwrap();
    if pat.is_match(name) {
        name.to_string()
    } else {
        name.to_string() + ext
    }
}

// Replace the suffix of the given name with the new one.
fn replace_suffix(name: &str, ext: &str) -> String {
    let pat = Regex::new(r"(.*)\.(dat|bak)\.gz$").unwrap();
    match pat.captures(name) {
        None => name.to_string() + ext,
        Some(cap) => cap.at(1).unwrap().to_string() + ext,
    }
}
