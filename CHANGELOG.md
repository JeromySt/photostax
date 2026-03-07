# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-03-07

### Fixed

- Release workflow: package platform archives to avoid asset name collisions
- Release workflow: npm version command `--allow-same-version` flag
- Release workflow: switch npm publish to OIDC Trusted Publishing
- CI: pin `dtolnay/rust-toolchain` and `taiki-e/install-action` to commit SHAs
- CI: correct YAML indentation for toolchain `with:` blocks
- CI: exclude `bindings/typescript` from Cargo workspace
- CI: bump `actions/cache` v5, `actions/upload-artifact` v7, `actions/download-artifact` v8

## [0.1.0] - 2026-02-24

### Added

- Initial release of `photostax-core` library
  - `PhotoStack` abstraction for grouping front, enhanced, and back scans
  - `Repository` trait with `LocalRepository` implementation
  - EXIF metadata reading support
  - XMP sidecar files for custom metadata and EXIF overrides

- Initial release of `photostax-cli` tool
  - Directory scanning for photo stacks
  - Basic inspection commands

- Initial release of `photostax-ffi` C bindings
  - Repository creation and scanning
  - Version information

[0.1.1]: https://github.com/JeromySt/photostax/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/JeromySt/photostax/releases/tag/v0.1.0
