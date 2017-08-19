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

// To compute hashing speed, use 1 over the benchmark time in seconds, and
// then multiply the result by the number of iterations in the 'for i'
// loop.  For example, if the benchmark runs in 29,924,583 ns/iter, and the
// count is 16, that would be about 534 MiB/sec hash performance.
//
// The loop count should be large enough to overflow the CPU's largest
// cache, with the value 16 (16MiB) overflowing the 8MiB cache on the Core
// i7-950 I wrote this on.
#[bench]
fn tree_mb_bench(b: &mut Bencher) {
    let tmp = TempDir::new("rsure-bench").unwrap();
    for i in 0..16 {
        let name = format!("large-{}", i);
        let mut fd = File::create(tmp.path().join(&name)).unwrap();
        let buf = vec![0; 1024];
        for _ in 0..1024 {
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
