// Benchmark our hashing function.

#![feature(test)]

extern crate rsure;
extern crate tempdir;
extern crate test;

use rsure::{Progress, SureHash};
use tempdir::TempDir;
use std::fs::File;
use std::io::Write;
use test::Bencher;

// To compute hashing speed, use 1 over the benchmark time in seconds.  For
// example, if the benchmark runs in 1,863,225 ns/iter, that would be about
// 536 MiB/sec hash performance.
#[bench]
fn tree_mb_bench(b: &mut Bencher) {
    let tmp = TempDir::new("rsure-bench").unwrap();
    {
        let mut fd = File::create(tmp.path().join("large")).unwrap();
        let buf = vec![0; 1024];
        for _ in 0 .. 1024 {
            fd.write_all(&buf).unwrap();
        }
    }

    b.iter(|| {
        let mut tree = rsure::scan_fs(tmp.path()).unwrap();
        let estimate = tree.hash_estimate();
        let mut progress = Progress::new(estimate.files, estimate.bytes);
        tree.hash_update(tmp.path(), &mut progress);
        // progress.flush();
    })
}
