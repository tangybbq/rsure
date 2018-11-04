// Benchmark our hashing function.

#![feature(test)]

extern crate openssl;
extern crate rsure;
extern crate tempdir;
extern crate test;
// extern crate sha1;

use rsure::{Progress, SureHash};
use std::fs::File;
use std::io::Write;
use tempdir::TempDir;
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

#[bench]
fn openssl_bench(b: &mut Bencher) {
    use openssl::hash::{Hasher, MessageDigest};

    // Make buffer big enough to not fit in cache.
    let buf = vec![0; 1024 * 1024 * 16];

    b.iter(|| {
        let mut h = Hasher::new(MessageDigest::sha1()).unwrap();
        h.write_all(&buf).unwrap();
        h.finish().unwrap();
    })
}

/* Bring in the SHA1 crate.  Currently, it seems to be about 4.2 times slower than the openssl one.
 */
/*
#[bench]
fn sha1_bench(b: &mut Bencher) {
    use sha1::Sha1;

    // Make buffer big enough to not fit in cache.
    let buf = vec![0; 1024 * 1024 * 16];

    b.iter(|| {
        let mut h = Sha1::new();
        h.update(&buf);
        let _ = h.digest();
    })
}
*/
