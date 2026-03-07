# Epson FastFoto Naming Convention

This document explains how Epson FastFoto scanners name their output files and how photostax interprets them.

## Overview

Epson FastFoto scanners (FF-640, FF-680W, etc.) produce multiple files per scanned photo. The scanner uses a consistent naming convention to distinguish between different versions of the same photo.

## File Naming Pattern

```
<Prefix>_<Number>[_<Suffix>].<Extension>
```

| Component | Description | Example |
|-----------|-------------|---------|
| Prefix | User-configurable name prefix | `Photo`, `IMG`, `Scan` |
| Number | Sequential 4-digit number | `0001`, `0042`, `1234` |
| Suffix | Optional version indicator | `_a` (enhanced), `_b` (back) |
| Extension | File format extension | `.jpg`, `.tif` |

## File Types

### Original Scan (No Suffix)

```
Photo_0001.jpg
IMG_0042.tif
```

The base file with no suffix is the **original front scan** — the unprocessed output from the scanner's CCD sensor. This file preserves the exact pixels captured by the scanner.

### Enhanced Version (`_a` Suffix)

```
Photo_0001_a.jpg
IMG_0042_a.tif
```

Files with the `_a` suffix are the **enhanced (color-corrected) version**. The scanner's software applies automatic adjustments:

- Auto-exposure correction
- Color balance optimization
- Dust and scratch removal (if enabled)
- Red-eye reduction (if enabled)

### Back Scan (`_b` Suffix)

```
Photo_0001_b.jpg
IMG_0042_b.tif
```

Files with the `_b` suffix contain the **back of the photo**. This is only produced when:

1. Back scanning is enabled in scanner settings
2. The scanner detects content on the back (text, dates, etc.)

Back scans are valuable for:
- Reading handwritten notes or dates
- OCR processing for searchable text
- Preserving historical context

## Format Detection

photostax recognizes files by extension (case-insensitive):

| Format | Extensions |
|--------|------------|
| JPEG | `.jpg`, `.jpeg` |
| TIFF | `.tif`, `.tiff` |

## Grouping Algorithm

photostax groups files into PhotoStacks using this algorithm:

```
1. For each file in directory:
   a. Parse filename into (prefix, number, suffix, extension)
   b. Compute base_id = "{prefix}_{number}"
   
2. Group files by base_id:
   a. No suffix → original
   b. "_a" suffix → enhanced
   c. "_b" suffix → back
   
3. Create PhotoStack for each group with ≥1 file
```

### Example

Given these files:
```
Photo_0001.jpg
Photo_0001_a.jpg
Photo_0001_b.jpg
Photo_0002.tif
Photo_0002_a.tif
```

photostax produces two PhotoStacks:

| Stack ID | Original | Enhanced | Back |
|----------|----------|----------|------|
| `Photo_0001` | `Photo_0001.jpg` | `Photo_0001_a.jpg` | `Photo_0001_b.jpg` |
| `Photo_0002` | `Photo_0002.tif` | `Photo_0002_a.tif` | — |

## Edge Cases

### Mixed Formats

If the scanner saves in both JPEG and TIFF (uncommon), photostax creates separate stacks:

```
Photo_0001.jpg  → Stack "Photo_0001" (JPEG)
Photo_0001.tif  → Stack "Photo_0001" (TIFF)
```

This is rare but can occur if scanner settings change mid-scan.

### Missing Files

PhotoStacks can have any combination of files:

| original | enhanced | back | Valid? |
|----------|----------|------|--------|
| ✓ | ✓ | ✓ | ✓ Complete stack |
| ✓ | ✓ | — | ✓ No back scan |
| ✓ | — | — | ✓ Original only |
| — | ✓ | — | ✓ Enhanced only |
| — | — | ✓ | ✓ Back only (rare) |

### Non-FastFoto Files

Files that don't match the pattern are ignored:

```
random_photo.jpg     → Ignored (no number)
Photo0001.jpg        → Ignored (missing underscore)
Photo_abc.jpg        → Ignored (non-numeric)
```

## Scanner Settings Reference

| Setting | Effect on Output |
|---------|------------------|
| File Name Prefix | Changes the `<Prefix>` component |
| Save Photo Enhancement | Enables `_a` file generation |
| Scan Backs | Enables `_b` file generation |
| File Format | Sets JPEG or TIFF output |
| JPEG Quality | Compression level (1-100) |
| TIFF Compression | None, LZW, or ZIP |

---

[← Back to main README](../README.md) | [Architecture →](architecture.md)
