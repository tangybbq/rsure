// Playing with paths.

extern crate flate2;
extern crate rustc_serialize;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate log;

use clap::{App, Arg, SubCommand};

use std::error;
use std::result;

pub type Result<T> = result::Result<T, Box<error::Error + Send + Sync>>;

mod escape;
mod show;
mod suretree;

#[allow(dead_code)]
fn main() {
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

    let file = matches.value_of("file").unwrap_or("2sure");
    let src = matches.value_of("src");

    match matches.subcommand() {
        ("scan", Some(_)) => {
            println!("scan: {}", file);
        },
        ("update", Some(_)) => {
            println!("udpate: {:?} -> {}", src, file);
        },
        ("check", Some(_)) => {
            println!("check {}", file);
        },
        ("signoff", Some(_)) => {
            println!("signoff {:?} -> {}", src, file);
        },
        ("show", Some(_)) => {
            println!("show {}", file);
            show::show(file).unwrap();
        },
        _ => {
            panic!("Unsupported command.");
        }
    }
}
