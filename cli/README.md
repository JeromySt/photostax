# photostax-cli

Command-line tool for inspecting and managing Epson FastFoto photo stacks.

## Installation

```sh
cargo install --path cli
```

Or build from source:

```sh
cargo build --release --package photostax-cli
```

## Quick Start

```sh
# List all photo stacks in a directory
photostax-cli scan /photos

# Search for stacks containing "birthday"
photostax-cli search /photos birthday

# Show details about a specific stack
photostax-cli info /photos IMG_0001

# Export all stacks as JSON
photostax-cli export /photos --output stacks.json
```

## Commands

### `scan` - List photo stacks

Scan a directory for Epson FastFoto photo stacks and display them.

```sh
photostax-cli scan <DIRECTORY> [OPTIONS]
```

**Options:**
- `-f, --format <FORMAT>` — Output format: `table` (default), `json`, `csv`
- `--show-metadata` — Include metadata in output
- `--tiff-only` — Only show TIFF stacks
- `--jpeg-only` — Only show JPEG stacks
- `--with-back` — Only show stacks with back scans

**Examples:**

```sh
# List all stacks
photostax-cli scan /photos

# List stacks as JSON
photostax-cli scan /photos --format json

# Only show stacks with back scans
photostax-cli scan /photos --with-back

# Show TIFF stacks with metadata
photostax-cli scan /photos --tiff-only --show-metadata
```

### `search` - Search by metadata

Search photo stacks by text query and metadata filters.

```sh
photostax-cli search <DIRECTORY> <QUERY> [OPTIONS]
```

**Options:**
- `--exif <KEY=VALUE>` — Filter by EXIF tag (can be repeated)
- `--tag <KEY=VALUE>` — Filter by custom tag (can be repeated)
- `--has-back` — Only stacks with back scans
- `--has-enhanced` — Only stacks with enhanced scans
- `-f, --format <FORMAT>` — Output format

**Examples:**

```sh
# Search for stacks containing "birthday"
photostax-cli search /photos birthday

# Search with EXIF filter
photostax-cli search /photos "" --exif Make=EPSON

# Search with back scan requirement
photostax-cli search /photos family --has-back

# Search with custom tag
photostax-cli search /photos "" --tag album=Vacation
```

### `info` - Show stack details

Display comprehensive information about a single photo stack.

```sh
photostax-cli info <DIRECTORY> <STACK_ID> [OPTIONS]
```

**Options:**
- `-f, --format <FORMAT>` — Output format

**Examples:**

```sh
# Show stack info
photostax-cli info /photos IMG_0001

# Show as JSON
photostax-cli info /photos IMG_0001 --format json
```

**Output includes:**
- All file paths and sizes
- EXIF tags from image files
- XMP metadata
- Custom tags from sidecar database

### `metadata` - Manage metadata

Read or write metadata for a photo stack.

#### `metadata read`

Display all metadata for a photo stack.

```sh
photostax-cli metadata read <DIRECTORY> <STACK_ID> [OPTIONS]
```

**Options:**
- `-f, --format <FORMAT>` — Output format

**Examples:**

```sh
photostax-cli metadata read /photos IMG_0001
photostax-cli metadata read /photos IMG_0001 --format json
```

#### `metadata write`

Add or update custom tags for a photo stack. Tags are stored in the sidecar database (`.photostax.db`) and do not modify the original image files.

```sh
photostax-cli metadata write <DIRECTORY> <STACK_ID> --tag <KEY=VALUE>...
```

**Examples:**

```sh
# Add a single tag
photostax-cli metadata write /photos IMG_0001 --tag album="Family Photos"

# Add multiple tags
photostax-cli metadata write /photos IMG_0001 \
  --tag album="Vacation 2024" \
  --tag location=Hawaii \
  --tag people="John, Jane"
```

#### `metadata delete`

Remove custom tags from a photo stack.

```sh
photostax-cli metadata delete <DIRECTORY> <STACK_ID> --tag <KEY>...
```

**Examples:**

```sh
# Delete a single tag
photostax-cli metadata delete /photos IMG_0001 --tag temp_tag

# Delete multiple tags
photostax-cli metadata delete /photos IMG_0001 --tag tag1 --tag tag2
```

### `export` - Export as JSON

Export all photo stacks with full metadata as JSON.

```sh
photostax-cli export <DIRECTORY> [OPTIONS]
```

**Options:**
- `-o, --output <FILE>` — Output file (default: stdout)

**Examples:**

```sh
# Export to stdout
photostax-cli export /photos

# Export to file
photostax-cli export /photos --output stacks.json

# Pipe to another tool
photostax-cli export /photos | jq '.[] | .id'
```

## Output Formats

### Table (default)

Human-readable table with unicode box-drawing characters:

```
┌────────────┬─────────┬──────────┬──────┬──────┬────────┐
│ ID         │ Format  │ Original │ Enh. │ Back │ Tags   │
├────────────┼─────────┼──────────┼──────┼──────┼────────┤
│ IMG_0001   │ JPEG    │    ✓     │  ✓   │  ✓   │     12 │
│ IMG_0002   │ JPEG    │    ✓     │  ✓   │  -   │      4 │
└────────────┴─────────┴──────────┴──────┴──────┴────────┘
```

### JSON

Valid, pretty-printed JSON:

```json
[
  {
    "id": "IMG_0001",
    "original": "/photos/IMG_0001.jpg",
    "enhanced": "/photos/IMG_0001_a.jpg",
    "back": "/photos/IMG_0001_b.jpg",
    "metadata": {
      "exif_tags": { "Make": "EPSON", ... },
      "custom_tags": { "album": "Family" }
    }
  }
]
```

### CSV

Comma-separated values for spreadsheets:

```csv
id,format,has_original,has_enhanced,has_back,tag_count
IMG_0001,jpeg,true,true,true,12
IMG_0002,jpeg,true,true,false,4
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Error (I/O, database, etc.) |
| 2 | Not found (stack ID doesn't exist) |

## FastFoto File Naming Convention

Epson FastFoto creates files with this naming convention:

| File | Description |
|------|-------------|
| `<name>.jpg` or `<name>.tif` | Original front scan |
| `<name>_a.jpg` or `<name>_a.tif` | Enhanced (color-corrected) |
| `<name>_b.jpg` or `<name>_b.tif` | Back of photo |

These related files are grouped into a "stack" for unified management.

## License

Licensed under either of [Apache License, Version 2.0](../LICENSE-APACHE) or [MIT License](../LICENSE-MIT) at your option.
