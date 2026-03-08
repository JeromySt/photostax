# photostax-cli

**CLI tool for inspecting and managing Epson FastFoto photo stacks.**

[![Crates.io](https://img.shields.io/crates/v/photostax-cli.svg)](https://crates.io/crates/photostax-cli)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/JeromySt/photostax#license)

## Installation

```sh
cargo install photostax-cli
```

Or build from source:

```sh
git clone https://github.com/JeromySt/photostax
cd photostax
cargo build --release --package photostax-cli
```

## Quick Start

```sh
# Scan a directory for photo stacks
photostax-cli scan /path/to/photos

# Get info about a specific photo stack
photostax-cli info /path/to/photos IMG_0001

# Search for photos with text
photostax-cli search /path/to/photos "birthday"
```

## Commands

### scan

Scan a directory and list all photo stacks.

```sh
photostax-cli scan /photos
photostax-cli scan /photos --format json
photostax-cli scan /photos --recursive
photostax-cli scan /photos --limit 20 --offset 0
```

### info

Show detailed information about a specific photo stack.

```sh
photostax-cli info /photos IMG_0001
photostax-cli info /photos IMG_0001 --format json
```

### search

Search for stacks matching text or filters.

```sh
photostax-cli search /photos "vacation"
photostax-cli search /photos --has-back
photostax-cli search /photos --exif Make=EPSON
photostax-cli search /photos "vacation" --limit 10 --offset 20
```

### export

Export all stacks to a JSON file.

```sh
photostax-cli export /photos --output stacks.json
photostax-cli export /photos --output stacks.json --include-metadata
```

### metadata

Read or write metadata for a photo stack.

```sh
# Read metadata
photostax-cli metadata read /photos IMG_0001
photostax-cli metadata read /photos IMG_0001 --format json

# Write metadata
photostax-cli metadata write /photos IMG_0001 --tag album="Family Photos"
photostax-cli metadata write /photos IMG_0001 --tag people="John,Jane"
```

## Features

- **Scan directories** for FastFoto photo stacks (JPEG and TIFF)
- **Inspect metadata** — View EXIF, XMP, and custom tags
- **Search & filter** — Query by text, metadata, or stack properties
- **Pagination** — Use `--limit` and `--offset` to page through results
- **Export** — Generate JSON reports for scripting
- **Metadata management** — Read and write custom tags

## Output Formats

| Format | Flag | Description |
|--------|------|-------------|
| Table | (default) | Human-readable table output |
| JSON | `--format json` | Machine-readable JSON |
| CSV | `--format csv` | Comma-separated values |

## Building from Source

```sh
git clone https://github.com/JeromySt/photostax
cd photostax
cargo build --release --package photostax-cli

# Run tests
cargo test --package photostax-cli
```

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or [MIT License](../LICENSE-MIT) at your option.

---

[← Back to main README](../README.md) | [Core Library](../core/README.md)
