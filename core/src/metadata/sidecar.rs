// TODO: SQLite sidecar database for extended metadata
//
// Planned: create a `.photostax.db` file alongside photo directories using
// `rusqlite`. Provides CRUD for custom key-value metadata per PhotoStack,
// enabling storage of OCR results, processing flags, relationships, and
// user-defined fields that don't fit in standard EXIF/IPTC/XMP tags.
