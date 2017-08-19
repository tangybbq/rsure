/// A simple progress meter.
///
/// Records updates of number of files visited, and number of bytes
/// processed.  When given an estimate, printes a simple periodic report of
/// how far along we think we are.

use time::{Duration, Timespec, get_time};

pub struct Progress {
    next_update: Timespec,

    cur_files: u64,
    total_files: u64,

    cur_bytes: u64,
    total_bytes: u64,
}

impl Progress {
    /// Construct a progress meter, with the given number of files and
    /// bytes as an estimate.
    pub fn new(files: u64, bytes: u64) -> Progress {
        Progress {
            cur_files: 0,
            total_files: files,

            cur_bytes: 0,
            total_bytes: bytes,

            next_update: get_time() + Duration::seconds(5),
        }
    }

    /// Update the progress meter.
    pub fn update(&mut self, files: u64, bytes: u64) {
        self.cur_files += files;
        self.cur_bytes += bytes;

        if get_time() > self.next_update {
            self.flush();
        }
    }

    /// Flush the output, regardless of if any update is needed.
    pub fn flush(&mut self) {
        println!(
            "{:7}/{:7} ({:5.1}%) files, {}/{} ({:5.1}%) bytes",
            self.cur_files,
            self.total_files,
            (self.cur_files as f64 * 100.0) / self.total_files as f64,
            humanize(self.cur_bytes),
            humanize(self.total_bytes),
            (self.cur_bytes as f64 * 100.0) / self.total_bytes as f64
        );

        self.next_update = get_time() + Duration::seconds(5);
    }
}

/// Print a size in a more human-friendly format.
pub fn humanize(value: u64) -> String {
    let mut value = value as f64;
    let mut unit = 0;

    while value > 1024.0 {
        value /= 1024.0;
        unit += 1;
    }

    static UNITS: [&'static str; 9] = [
        "B  ",
        "KiB",
        "MiB",
        "GiB",
        "TiB",
        "PiB",
        "EiB",
        "ZiB",
        "YiB",
    ];

    let precision = if value < 10.0 {
        3
    } else if value < 100.0 {
        2
    } else {
        1
    };

    format!("{:6.*}{}", precision, value, UNITS[unit])
}
