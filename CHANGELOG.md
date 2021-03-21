# ChangeLog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0),
and this project adheres to [Semantic Versioning](https://semver.og/spec/v2.0.0.html).

## [Unreleased]

## [0.9.3]

### Changed

- This release has no code changes, and is merely a version bump to
  properly tag this and release to [crates.io](https://crates.io/).

## [0.9.1]

### Changed

- Weave parser now implements a pull parser.  This avoids the overhead
  of threads and channels for normal processing of the surefie.
- Numerous minor code cleanups from clippy and rustfmt
- Add `default.nix` and `shell.nix` to help with development under
  Nix.

### Fixed

- Fix duplicated names in some comparison messages
