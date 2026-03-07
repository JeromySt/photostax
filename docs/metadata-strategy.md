# Metadata Strategy

This document describes how photostax reads, writes, and manages photo metadata.

## Overview

photostax uses a three-tier metadata system:

| Tier | Source | Access | Interoperability |
|------|--------|--------|------------------|
| **EXIF** | Image files | Read-only | Universal |
| **XMP** | Image files / sidecars | Read/Write | High (Adobe standard) |
| **XMP Sidecar** | `.xmp` files | Read/Write | High (Adobe standard) |

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

## XMP Sidecar Files

For application-specific metadata that doesn't map to standard XMP properties
(custom tags, EXIF overrides), photostax stores data in XMP sidecar files
alongside the images.

### Sidecar File Layout

Each photo stack gets a single `.xmp` file named after the stack ID:

```
/photos/
├── IMG_001.jpg
├── IMG_001_a.jpg
├── IMG_001_b.jpg
├── IMG_001.xmp           ← XMP sidecar for the stack
├── IMG_002.tif
└── IMG_002.xmp
```

### Data Stored in Sidecar

| Category | Namespace | Example |
|----------|-----------|---------|
| Standard XMP | `dc:` | `dc:description`, `dc:creator` |
| Custom tags | `photostax:customTags` | JSON blob of key-value pairs |
| EXIF overrides | `photostax:exifOverrides` | JSON blob of key-value pairs |

### Advantages Over a Database

| Benefit | Description |
|---------|-------------|
| **Portable** | `.xmp` files travel with images when copied/moved |
| **Interoperable** | Adobe Lightroom, darktable, and other apps read `.xmp` files |
| **No dependencies** | No database engine required |
| **Inspectable** | Plain XML, human-readable |
| **Backup-friendly** | Standard file backup tools capture sidecars automatically |

## Tag Priority and Merging

When loading metadata, photostax merges from multiple sources with this priority:

```
Priority: XMP sidecar > Embedded XMP > EXIF (for shared keys)
```

### Merge Algorithm

```python
def merge_metadata(exif, embedded_xmp, sidecar_xmp):
    result = {}
    
    # 1. Start with EXIF (lowest priority for editable tags)
    result.update(exif.tags)
    
    # 2. Override with embedded XMP
    result.update(embedded_xmp.properties)
    
    # 3. Override with sidecar XMP (highest priority)
    result.update(sidecar_xmp.xmp_tags)
    
    # 4. Apply EXIF overrides from sidecar
    result.update(sidecar_xmp.exif_overrides)
    
    # 5. Always include custom tags from sidecar (no conflict possible)
    result['custom'] = sidecar_xmp.custom_tags
    
    return result
```

### Conflict Resolution

| Tag | EXIF | XMP | Result |
|-----|------|-----|--------|
| Description | "Scanned photo" | "Beach vacation 2020" | "Beach vacation 2020" (XMP wins) |
| DateTime | "2024:01:15 10:30:00" | — | "2024:01:15 10:30:00" (EXIF preserved) |
| Rating | — | "4" | "4" (XMP-only tag) |
| People | — | — | ["John", "Jane"] (sidecar custom tag) |

## Writing Metadata

### Write Operation Flow

```
1. Application calls repo.write_metadata(stack, metadata)

2. For all properties (XMP, custom tags, EXIF overrides):
   a. Read-modify-write the stack's XMP sidecar file (.xmp)
   b. Standard XMP keys go into dc: namespace
   c. Custom tags serialized as JSON in photostax:customTags
   d. EXIF overrides serialized as JSON in photostax:exifOverrides
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
2. **Use custom tags for app-specific data**: OCR text, faces, internal IDs stored in sidecar XMP
3. **Never modify EXIF**: Preserves original scanner data; use EXIF overrides instead
4. **Back up sidecar files**: `.xmp` files contain all custom metadata
5. **Use keywords for searchability**: Indexed by most photo apps

## AI / ML Integration

photostax exposes its metadata write-back API for use by any AI, ML, or external
analysis tool.  There is no separate "AI metadata" namespace — AI is just another
writer that uses the same standard fields.  This ensures that metadata produced by
AI assessment is immediately readable by **every** standard photo application
(Lightroom, darktable, Google Photos, Apple Photos, etc.).

### How AI Should Write Metadata

1. **Use `xmp_tags` for standard Dublin Core fields** — these are embedded directly
   into JPEG files and into the XMP sidecar, readable by any viewer.
2. **Use `custom_tags` for structured / rich data** — arrays, objects, and any data
   that doesn't fit a flat string.  These live in the XMP sidecar under the
   `photostax:customTags` namespace.
3. **Use `write_metadata()`** — the single entry-point that handles embedding, sidecar
   creation, and merge logic.

### Dublin Core Mapping (xmp_tags)

These keys are automatically mapped to the universally-recognised `dc:` namespace:

| `xmp_tags` Key | XMP Property | AI Use Case |
|----------------|--------------|-------------|
| `description` | `dc:description` | AI-generated caption / scene description |
| `title` | `dc:title` | Suggested photo title |
| `subject` | `dc:subject` | Keywords: people, places, objects, events (comma-separated) |
| `creator` | `dc:creator` | Photographer or AI attribution |
| `date` | `dc:date` | AI-estimated original photo date |
| `rights` | `dc:rights` | Copyright notice |

Any key **not** in the above list (e.g. `scene`, `estimated_time`) is stored in the
`photostax:` namespace in XMP — still standards-compliant and visible in advanced
viewers.

### Structured Custom Tags (custom_tags)

For richer data that benefits from JSON types, write to `custom_tags`:

| Key | JSON Value | Description |
|-----|------------|-------------|
| `people` | `["Alice", "Bob"]` | People identified in the photo |
| `places` | `["Paris", "Eiffel Tower"]` | Named locations or landmarks |
| `location` | `{"lat": 48.8, "lng": 2.3}` | Geo-coordinates |
| `events` | `["Wedding"]` | Events depicted |
| `holidays` | `["Christmas"]` | Holidays or celebrations detected |
| `era` | `"1980s"` | Estimated decade |
| `mood` | `"joyful"` | Emotional tone of the photo |
| `scene` | `"outdoor beach at sunset"` | Scene classification |
| `objects` | `["dog", "car"]` | Notable objects detected |
| `ocr_front` | `"Hello World"` | OCR text from front of photo |
| `ocr_back` | `"Happy Birthday!"` | OCR text from back of photo |
| `caption` | `"A family of four on the beach"` | Long-form AI caption |
| `colors` | `["blue", "green"]` | Dominant colours |
| `confidence` | `0.92` | Overall AI confidence score |

### Example: AI Enrichment Flow

```rust
use photostax_core::photo_stack::Metadata;

let mut metadata = Metadata::default();

// Standard XMP — written to the image file and sidecar, readable everywhere
metadata.xmp_tags.insert("description".into(), "Family at the beach".into());
metadata.xmp_tags.insert("subject".into(), "beach, family, Alice, Bob".into());
metadata.xmp_tags.insert("date".into(), "1985-07-04".into());

// Structured data — stored in the XMP sidecar as JSON
metadata.custom_tags.insert("people".into(), serde_json::json!(["Alice", "Bob"]));
metadata.custom_tags.insert("events".into(), serde_json::json!(["Family Reunion"]));
metadata.custom_tags.insert("location".into(), serde_json::json!({"lat": 37.82, "lng": -122.48}));
metadata.custom_tags.insert("mood".into(), serde_json::json!("joyful"));

// Write once — embeds XMP in the JPEG + updates the .xmp sidecar
repo.write_metadata("IMG_0001", &metadata)?;
```

After this call, **any** photo viewer that reads Dublin Core XMP will see the
description, keywords, and date.  Applications that understand the `photostax:`
namespace (or simply parse the sidecar XML) can also read the structured custom tags.

---

[← Back to main README](../README.md) | [Architecture →](architecture.md)
