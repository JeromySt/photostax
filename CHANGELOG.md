# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-02-24

### Added

- Initial release of `photostax-core` library
  - `PhotoStack` abstraction for grouping front, enhanced, and back scans
  - `Repository` trait with `LocalRepository` implementation
  - EXIF metadata reading support
  - SQLite-backed caching for indexed lookups

- Initial release of `photostax-cli` tool
  - Directory scanning for photo stacks
  - Basic inspection commands

- Initial release of `photostax-ffi` C bindings
  - Repository creation and scanning
  - Version information

[0.1.0]: https://github.com/JeromySt/photostax/releases/tag/v0.1.0
