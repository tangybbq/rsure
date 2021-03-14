//! A simple progress meter.
//!
//! Records updates of number of files visited, and number of bytes
//! processed.  When given an estimate, printes a simple periodic report of
//! how far along we think we are.

use env_logger::Builder;
use lazy_static::lazy_static;
use log::Log;
use std::{
    io::{stdout, Write},
    sync::Mutex,
};
use time::{get_time, Duration, Timespec};

// The Rust logging system (log crate) only allows a single logger to be
// logged once.  If we want to capture this, it has to be done before any
// logger is initialized.  Globally, within a mutex, we keep this simple
// state of what is happening.
struct State {
    // The last message printed.  Since an empty string an no message are
    // the same thing, we don't worry about having an option here.
    message: String,

    // When we next expect to update the message.
    next_update: Timespec,

    // Set to true if the logging system has been initialized.
    is_logging: bool,
}

// The SafeLogger wraps another logger, coordinating the logging with the
// state to properly interleave logs and messages.
struct SafeLogger {
    inner: Box<dyn Log>,
}

/// Initialize the standard logger, based on `env_logger::init()`, but
/// coordinated with any progress meters.  Like `init`, this will panic if
/// the logging system has already been initialized.
pub fn log_init() {
    let mut st = STATE.lock().unwrap();
    let inner = Builder::from_default_env().build();
    let max_level = inner.filter();

    let logger = SafeLogger {
        inner: Box::new(inner),
    };
    log::set_boxed_logger(Box::new(logger)).expect("Set Logger");
    log::set_max_level(max_level);

    st.is_logging = true;
    st.next_update = update_interval(true);
}

// There are two update intervals, depending on whether we are logging.
fn update_interval(is_logging: bool) -> Timespec {
    if is_logging {
        get_time() + Duration::milliseconds(250)
    } else {
        get_time() + Duration::seconds(5)
    }
}

lazy_static! {
    // The current global state.
    static ref STATE: Mutex<State> = Mutex::new(State {
        message: String::new(),
        next_update: update_interval(false),
        is_logging: false,
    });
}

impl State {
    /// Called to advance to the next message, sets the update time
    /// appropriately.
    fn next(&mut self) {
        self.next_update = update_interval(self.is_logging);
    }

    /// Clears the visual text of the current message (but not the message
    /// buffer itself, so that it can be redisplayed if needed).
    fn clear(&self) {
        for ch in self.message.chars() {
            if ch == '\n' {
                print!("\x1b[1A\x1b[2K");
            }
        }
        stdout().flush().expect("safe stdout write");
    }

    /// Update the current message.
    fn update(&mut self, message: String) {
        self.clear();
        self.message = message;
        print!("{}", self.message);
        stdout().flush().expect("safe stdout write");
        self.next();
    }

    /// Indicates if the time has expired and another update should be
    /// done.  This can be used where the formatting/allocation of the
    /// update message would be slower than the possible system call needed
    /// to determine the current time.
    fn need_update(&self) -> bool {
        get_time() >= self.next_update
    }
}

impl Log for SafeLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &log::Record) {
        let enabled = self.inner.enabled(record.metadata());

        if enabled {
            let st = STATE.lock().unwrap();
            st.clear();
            self.inner.log(record);
            print!("{}", st.message);
            stdout().flush().expect("safe stdout write");
        }
    }

    fn flush(&self) {
        let st = STATE.lock().unwrap();
        st.clear();
        self.inner.flush();
        print!("{}", st.message);
        stdout().flush().expect("safe stdout write");
    }
}

pub struct Progress {
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
        }
    }

    /// Update the progress meter.
    pub fn update(&mut self, files: u64, bytes: u64) {
        self.cur_files += files;
        self.cur_bytes += bytes;

        let mut st = STATE.lock().unwrap();
        if st.need_update() {
            st.update(self.message());
        }
    }

    /// Flush the output, regardless of if any update is needed.
    pub fn flush(&mut self) {
        let mut st = STATE.lock().unwrap();
        st.update(self.message());

        // Clear the current message so that we don't clear out the shown
        // message.
        st.message.clear();
    }

    pub fn message(&self) -> String {
        format!(
            "{:7}/{:7} ({:5.1}%) files, {}/{} ({:5.1}%) bytes\n",
            self.cur_files,
            self.total_files,
            (self.cur_files as f64 * 100.0) / self.total_files as f64,
            humanize(self.cur_bytes),
            humanize(self.total_bytes),
            (self.cur_bytes as f64 * 100.0) / self.total_bytes as f64
        )
    }
}

/// A progress meter used when initially scanning.
pub struct ScanProgress {
    dirs: u64,
    files: u64,
    bytes: u64,
}

impl ScanProgress {
    /// Construct a new scanning progress meter.
    pub fn new() -> ScanProgress {
        ScanProgress {
            dirs: 0,
            files: 0,
            bytes: 0,
        }
    }

    /// Update the meter.
    pub fn update(&mut self, dirs: u64, files: u64, bytes: u64) {
        self.dirs += dirs;
        self.files += files;
        self.bytes += bytes;

        let mut st = STATE.lock().unwrap();
        if st.need_update() {
            st.update(self.message());
        }
    }

    fn message(&self) -> String {
        format!(
            "scan: {} dirs {} files, {} bytes\n",
            self.dirs,
            self.files,
            humanize(self.bytes)
        )
    }
}

impl Drop for ScanProgress {
    fn drop(&mut self) {
        let mut st = STATE.lock().unwrap();
        st.update(self.message());

        st.message.clear();
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

    static UNITS: [&str; 9] = [
        "B  ", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB",
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
