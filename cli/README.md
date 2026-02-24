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

## Usage

```sh
# Scan a directory for photo stacks
photostax-cli scan /path/to/photos

# Get info about a specific photo stack
photostax-cli info /path/to/photos/IMG001.jpg
```

## Features

- **Scan directories** for FastFoto photo stacks
- **Inspect metadata** — View EXIF data and stack composition
- **JSON output** — Machine-readable output for scripting

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or [MIT License](../LICENSE-MIT) at your option.

See the [main repository](https://github.com/JeromySt/photostax) for more details.
