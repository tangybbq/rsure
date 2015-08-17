// Playing with paths.

extern crate flate2;
extern crate rustc_serialize;
extern crate libc;
extern crate openssl;
extern crate env_logger;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate log;

use clap::{App, Arg, SubCommand};

use std::error;
use std::fs::rename;
use std::result;
use std::path::Path;
use surefs::scan_fs;
use hashes::SureHash;
use suretree::SureTree;
use comp::{TreeCompare, TreeUpdate};

pub type Result<T> = result::Result<T, Box<error::Error + Send + Sync>>;

mod escape;
mod show;
mod suretree;
mod surefs;
mod hashes;
mod comp;

#[allow(dead_code)]
fn main() {
    env_logger::init().unwrap();

    let matches = App::new("rsure")
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

    let file = remove_suffix(matches.value_of("file").unwrap_or("2sure"));
    let src = matches.value_of("src").map(|s| remove_suffix(s));
    let src = src.unwrap_or_else(|| file.clone());

    let tmp = file.clone() + ".tmp.gz";
    let file = file + ".dat.gz";
    let src = src + ".bak.gz";

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
            show::show(&Path::new(&file)).unwrap();
        },
        _ => {
            panic!("Unsupported command.");
        }
    }
}

fn remove_suffix(name: &str) -> String {
    if name.ends_with(".dat.gz") {
        name[0.. name.len() - 7].to_string()
    } else if name.ends_with(".bak.gz") {
        name[0.. name.len() - 7].to_string()
    } else if name.ends_with(".tmp.gz") {
        name[0.. name.len() - 7].to_string()
    } else {
        name.to_string()
    }
}
