# Rsure file integrity

It has been said that backups aren't useful unless you've tested them.
But, how does one know that a test restore actually worked?  Rsure is
designed to help with this.

## History

The md5sum program captures the MD5 hash of a set of files.  It can
also read this output and compare the hashes against the files.  By
capturing the hashes before the backup, and comparing them after a
test restore, you can gain a bit of confidence that the contents of
files is at least correct.

However, this doesn't capture the permissions and other attributes of
the files.  Sometimes a restore can fail for this kind of reason.

### Intrusion detection

There have been several similar solutions focused on intrusion
detection.  Tripwire and FreeVeracity (or Veracity) come to mind.  The
idea is that the files are compared in place to verify that nobody has
modified them.

Unfortunately, at least tripwire seems to focus so heavily on this
intrusion detection problem, that the tool doesn't work very well for
verifying backups.  It really wants a central database, and to use
files by absolute pathname.  FreeVeracity was quite useful for
verifying backups, however, it appears to have vanished entirely (it
was under an unusual license).

### Incremental updates

One thing that none of these solutions addressed was that of
incremental updates, probably because of the focus on intrusion
detection.  In a normal running system, the POSIX *ctime* field can be
reliably used to determine if a file has been modified.  By making use
of this, the integrity program can avoid recomputing hashes of files
that haven't changed.  This strategy is similar to what most backup
software does as well.  This is important, because taking the time to
hash every file can make the integrity update take so long that people
avoid running it.  Full hashing is impractical for the same reasons
that regular full backups are usually impractical.

# Using rsure

## Getting it

Rsure is written in [Rust](https://www.rust-lang.org/).  It began as
an exercise to determine how useful Rust is for a systems-type
program, and has shown to be the easiest implementation to develop and
maintain.

Once you have installed rust (and cargo) using either the rust
installer, rustup, or your distro's packaging system, building it is
as easy as:

```bash
$ cargo build --release
```

within the Rsure directory.  The `--release` flag is important,
otherwise the performance is poor.  You can install or link to
`./target/release/rsure` for the executable.  It may also be possible
to use `cargo install` to install sure directly.

## Basic usage

Change to a directory you wish to keep integrity for, for example, my
home directory:

```bash
$ cd
$ rsure scan
```

This will scan the filesystem (possibly showing progress), and leave a
'2sure.dat.gz' (the 2sure is historical, FreeVeracity used a name
starting with a 0, and having the digit makes it near the beginning of
a directory listing).  You can view this file if you'd like.  Aside
from being compressed, the format is plain ASCII (even if your
filenames are not).

Then you can do:

```bash
$ rsure check
```

to verify the directory.  This will show any differences.  If you back
up this file with your data, you can run `rsure` after a restore to
check if the backup is correct.

Later, you can run

```bash
$ rsure update
```

which will move the `2sure.dat.gz` file to `2sure.bak.gz`, and refresh
the hashes of any files that have changed.  After you have these two
files:

```bash
$ rsure signoff
```

will compare the old scan with the current, and report on what has
changed between them.
