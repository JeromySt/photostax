# Metadata Strategy

This document describes how photostax reads, writes, and manages photo metadata.

## Overview

photostax uses a three-tier metadata system:

| Tier | Source | Access | Interoperability |
|------|--------|--------|------------------|
| **EXIF** | Image files | Read-only | Universal |
| **XMP** | Image files / sidecars | Read/Write | High (Adobe standard) |
| **Sidecar DB** | SQLite database | Read/Write | photostax-only |

## EXIF Tags (Read-Only)

EXIF (Exchangeable Image File Format) data is embedded by the scanner during capture. photostax reads but never modifies EXIF data to preserve the original scan information.

### Standard Tags Read

| Tag | EXIF Name | Description |
|-----|-----------|-------------|
| Make | `Exif.Image.Make` | Scanner manufacturer (e.g., "EPSON") |
| Model | `Exif.Image.Model` | Scanner model (e.g., "FastFoto FF-680W") |
| DateTime | `Exif.Image.DateTime` | Scan timestamp |
| DateTimeOriginal | `Exif.Photo.DateTimeOriginal` | Original photo date (if set) |
| ImageDescription | `Exif.Image.ImageDescription` | User-provided description |
| XResolution | `Exif.Image.XResolution` | Horizontal DPI |
| YResolution | `Exif.Image.YResolution` | Vertical DPI |
| Software | `Exif.Image.Software` | Scanner software version |
| ColorSpace | `Exif.Photo.ColorSpace` | Color space (sRGB, AdobeRGB) |
| PixelXDimension | `Exif.Photo.PixelXDimension` | Image width |
| PixelYDimension | `Exif.Photo.PixelYDimension` | Image height |
| Orientation | `Exif.Image.Orientation` | Image rotation flag |

### GPS Tags (if present)

| Tag | EXIF Name | Description |
|-----|-----------|-------------|
| GPSLatitude | `Exif.GPSInfo.GPSLatitude` | Latitude coordinates |
| GPSLongitude | `Exif.GPSInfo.GPSLongitude` | Longitude coordinates |
| GPSAltitude | `Exif.GPSInfo.GPSAltitude` | Elevation |
| GPSDateStamp | `Exif.GPSInfo.GPSDateStamp` | GPS date |

## XMP Metadata (Read/Write)

XMP (Extensible Metadata Platform) is Adobe's standard for embedded metadata. photostax uses XMP as the primary writable metadata store because it's widely supported by photo applications.

### Why XMP?

1. **Interoperability**: Adobe Lightroom, Photoshop, Bridge, and many other apps read XMP
2. **Embedded in files**: Metadata travels with the image
3. **Extensible**: Supports custom namespaces
4. **Non-destructive**: Doesn't affect image pixels

### XMP Namespaces Used

| Namespace | Prefix | URI | Purpose |
|-----------|--------|-----|---------|
| Dublin Core | `dc` | `http://purl.org/dc/elements/1.1/` | Title, description, creator |
| XMP Basic | `xmp` | `http://ns.adobe.com/xap/1.0/` | Dates, software |
| IPTC Core | `Iptc4xmpCore` | `http://iptc.org/std/Iptc4xmpCore/1.0/xmlns/` | Location, keywords |
| photostax | `photostax` | `http://photostax.dev/ns/1.0/` | Custom stack metadata |

### Standard XMP Properties Written

| Property | XMP Path | Description |
|----------|----------|-------------|
| Title | `dc:title` | Photo title |
| Description | `dc:description` | Photo description |
| Creator | `dc:creator` | Photographer name |
| Subject | `dc:subject` | Keywords/tags array |
| Rating | `xmp:Rating` | Star rating (1-5) |
| Label | `xmp:Label` | Color label |
| CreateDate | `xmp:CreateDate` | Original photo date |

### Custom photostax Properties

| Property | XMP Path | Description |
|----------|----------|-------------|
| StackId | `photostax:StackId` | Unique stack identifier |
| Album | `photostax:Album` | Album membership |
| People | `photostax:People` | People in photo (array) |
| Event | `photostax:Event` | Event name |
| Notes | `photostax:Notes` | User notes |

### XMP Storage Locations

| Format | Location | Notes |
|--------|----------|-------|
| JPEG | Embedded in APP1 segment | Preferred, travels with file |
| TIFF | Embedded in TIFF tag | Same as JPEG |
| Sidecar | `<filename>.xmp` | Used when embedding fails |

## Sidecar Database

For metadata that doesn't fit XMP's model (complex structures, relationships, full-text search), photostax uses a SQLite sidecar database.

### Database Location

```
<repository_root>/.photostax/metadata.db
```

### Schema

```sql
-- Core metadata table
CREATE TABLE stacks (
    id TEXT PRIMARY KEY,           -- Stack ID (e.g., "Photo_0001")
    original_path TEXT,            -- Path to original file
    enhanced_path TEXT,            -- Path to enhanced file
    back_path TEXT,                -- Path to back file
    format TEXT,                   -- "jpeg" or "tiff"
    created_at TEXT,               -- First scan timestamp
    updated_at TEXT                -- Last modification
);

-- Custom key-value tags
CREATE TABLE custom_tags (
    stack_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT,                    -- JSON-encoded value
    PRIMARY KEY (stack_id, key),
    FOREIGN KEY (stack_id) REFERENCES stacks(id)
);

-- Full-text search index
CREATE VIRTUAL TABLE search_index USING fts5(
    stack_id,
    title,
    description,
    keywords,
    ocr_text,                      -- Text from back scans
    content='stacks'
);
```

### Use Cases for Sidecar DB

| Use Case | Why Not XMP? |
|----------|--------------|
| OCR text from backs | Large text blob, search index needed |
| Face detection data | Complex coordinates, not standardized |
| Relationship links | Album/collection membership |
| Full-text search | FTS5 index for fast queries |

## Tag Priority and Merging

When loading metadata, photostax merges from multiple sources with this priority:

```
Priority: XMP > EXIF > Sidecar DB (for shared keys)
```

### Merge Algorithm

```python
def merge_metadata(exif, xmp, sidecar):
    result = {}
    
    # 1. Start with sidecar (lowest priority for standard tags)
    result.update(sidecar.standard_tags)
    
    # 2. Override with EXIF (read from file)
    result.update(exif.tags)
    
    # 3. Override with XMP (highest priority for editable tags)
    result.update(xmp.properties)
    
    # 4. Always include custom sidecar tags (no conflict possible)
    result['custom'] = sidecar.custom_tags
    
    return result
```

### Conflict Resolution

| Tag | EXIF | XMP | Result |
|-----|------|-----|--------|
| Description | "Scanned photo" | "Beach vacation 2020" | "Beach vacation 2020" (XMP wins) |
| DateTime | "2024:01:15 10:30:00" | — | "2024:01:15 10:30:00" (EXIF preserved) |
| Rating | — | "4" | "4" (XMP-only tag) |
| People | — | — | ["John", "Jane"] (sidecar custom) |

## Writing Metadata

### Write Operation Flow

```
1. Application calls repo.writeMetadata(stackId, metadata)

2. For standard properties (title, description, keywords, etc.):
   a. Write to XMP in image file
   b. If write fails (permissions, format), write to sidecar XMP file
   
3. For custom properties:
   a. Store in sidecar SQLite database
   
4. Update search index
```

### Code Example

```rust
// Write metadata via the Repository
use photostax_core::photo_stack::Metadata;

let mut metadata = Metadata::default();
metadata.set_title("Beach Vacation 2020");
metadata.set_keywords(vec!["vacation", "beach", "family"]);
metadata.set_custom("album", serde_json::json!("Summer 2020"));
metadata.set_custom("people", serde_json::json!(["John", "Jane"]));

repo.write_metadata("Photo_0001", &metadata)?;
```

## Best Practices

1. **Use XMP for standard tags**: Ensures other apps can read them
2. **Use sidecar DB for app-specific data**: OCR, faces, internal IDs
3. **Never modify EXIF**: Preserves original scanner data
4. **Back up the sidecar database**: Contains custom metadata
5. **Use keywords for searchability**: Indexed by most photo apps

---

[← Back to main README](../README.md) | [Architecture →](architecture.md)
