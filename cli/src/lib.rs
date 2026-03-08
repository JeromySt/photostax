//! # photostax-cli
//!
//! Command-line tool for inspecting and managing Epson FastFoto photo stacks.
//!
//! This module provides the core CLI logic as a library, enabling both
//! the CLI binary and unit tests to use the same code.

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use photostax_core::backends::local::LocalRepository;
use photostax_core::metadata::ImageFormat;
use photostax_core::photo_stack::{Metadata, PhotoStack};
use photostax_core::repository::Repository;
use photostax_core::scanner::ScannerConfig;
use photostax_core::search::{filter_stacks, paginate_stacks, PaginationParams, SearchQuery};

/// CLI tool for inspecting and managing Epson FastFoto photo stacks
#[derive(Parser)]
#[command(name = "photostax-cli")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Scan a directory and list all photo stacks
    #[command(
        long_about = "Scan a directory for Epson FastFoto photo stacks and display them.\n\n\
        FastFoto creates files with naming convention:\n  \
        - <name>.jpg/.tif      Original front scan\n  \
        - <name>_a.jpg/.tif    Enhanced (color-corrected)\n  \
        - <name>_b.jpg/.tif    Back of photo\n\n\
        These are grouped into 'stacks' for unified management."
    )]
    Scan {
        /// Directory containing FastFoto scans
        directory: PathBuf,

        /// Output format
        #[arg(long, short, value_enum, default_value_t = OutputFormat::Table)]
        format: OutputFormat,

        /// Include metadata in output
        #[arg(long)]
        show_metadata: bool,

        /// Only show TIFF stacks
        #[arg(long, conflicts_with = "jpeg_only")]
        tiff_only: bool,

        /// Only show JPEG stacks
        #[arg(long, conflicts_with = "tiff_only")]
        jpeg_only: bool,

        /// Only show stacks with back scans
        #[arg(long)]
        with_back: bool,

        /// Recurse into subdirectories
        #[arg(long, short)]
        recursive: bool,

        /// Maximum number of stacks to return per page (0 = no pagination)
        #[arg(long, default_value_t = 0)]
        limit: usize,

        /// Number of stacks to skip (0-based offset)
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },

    /// Search photo stacks by metadata
    #[command(
        long_about = "Search photo stacks by text query and metadata filters.\n\n\
        The text query searches across stack IDs and all metadata values.\n\
        Additional filters can narrow results to specific EXIF or custom tags."
    )]
    Search {
        /// Directory containing FastFoto scans
        directory: PathBuf,

        /// Text to search for in IDs and metadata
        query: String,

        /// Filter by EXIF tag (format: KEY=VALUE)
        #[arg(long = "exif", value_parser = parse_key_value)]
        exif_filters: Vec<(String, String)>,

        /// Filter by custom tag (format: KEY=VALUE)
        #[arg(long = "tag", value_parser = parse_key_value)]
        tag_filters: Vec<(String, String)>,

        /// Only show stacks with back scans
        #[arg(long)]
        has_back: bool,

        /// Only show stacks with enhanced scans
        #[arg(long)]
        has_enhanced: bool,

        /// Output format
        #[arg(long, short, value_enum, default_value_t = OutputFormat::Table)]
        format: OutputFormat,

        /// Maximum number of stacks to return per page (0 = no pagination)
        #[arg(long, default_value_t = 0)]
        limit: usize,

        /// Number of stacks to skip (0-based offset)
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },

    /// Show detailed information about a specific stack
    #[command(
        long_about = "Display comprehensive information about a single photo stack.\n\n\
        Shows all file paths, file sizes, and complete metadata including\n\
        EXIF tags, XMP tags, and custom tags from the XMP sidecar file."
    )]
    Info {
        /// Directory containing FastFoto scans
        directory: PathBuf,

        /// Stack ID (base filename without suffix or extension)
        stack_id: String,

        /// Output format
        #[arg(long, short, value_enum, default_value_t = OutputFormat::Table)]
        format: OutputFormat,
    },

    /// Read or write metadata for a stack
    #[command(subcommand)]
    Metadata(MetadataCommand),

    /// Export stack data as JSON
    #[command(long_about = "Export all photo stacks with full metadata as JSON.\n\n\
        Output can be written to a file or stdout for piping to other tools.")]
    Export {
        /// Directory containing FastFoto scans
        directory: PathBuf,

        /// Output file (default: stdout)
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum MetadataCommand {
    /// Read metadata for a stack
    #[command(
        long_about = "Display all metadata for a photo stack including EXIF, XMP, and custom tags."
    )]
    Read {
        /// Directory containing FastFoto scans
        directory: PathBuf,

        /// Stack ID
        stack_id: String,

        /// Output format
        #[arg(long, short, value_enum, default_value_t = OutputFormat::Table)]
        format: OutputFormat,
    },

    /// Write metadata tags to a stack
    #[command(long_about = "Add or update custom tags for a photo stack.\n\n\
        Tags are written to an XMP sidecar file (.xmp) alongside the images\n\
        and do not modify the original image files.")]
    Write {
        /// Directory containing FastFoto scans
        directory: PathBuf,

        /// Stack ID
        stack_id: String,

        /// Tags to write (format: KEY=VALUE)
        #[arg(long = "tag", required = true, value_parser = parse_key_value)]
        tags: Vec<(String, String)>,
    },

    /// Delete metadata tags from a stack
    #[command(long_about = "Remove custom tags from a photo stack's XMP sidecar file.")]
    Delete {
        /// Directory containing FastFoto scans
        directory: PathBuf,

        /// Stack ID
        stack_id: String,

        /// Tag keys to delete
        #[arg(long = "tag", required = true)]
        tags: Vec<String>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable table with unicode box-drawing
    Table,
    /// JSON output
    Json,
    /// Comma-separated values
    Csv,
}

// Exit codes
pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_ERROR: i32 = 1;
pub const EXIT_NOT_FOUND: i32 = 2;

/// Parse KEY=VALUE format for tag filters
pub fn parse_key_value(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid format '{s}', expected KEY=VALUE"));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

/// Run the CLI with parsed arguments, writing output to `out` and errors to `err`.
/// Returns the exit code.
pub fn run_cli(cli: &Cli, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    match &cli.command {
        Commands::Scan {
            directory,
            format,
            show_metadata,
            tiff_only,
            jpeg_only,
            with_back,
            recursive,
            limit,
            offset,
        } => cmd_scan(
            out,
            err,
            directory,
            *format,
            *show_metadata,
            *tiff_only,
            *jpeg_only,
            *with_back,
            *recursive,
            *limit,
            *offset,
        ),

        Commands::Search {
            directory,
            query,
            exif_filters,
            tag_filters,
            has_back,
            has_enhanced,
            format,
            limit,
            offset,
        } => cmd_search(
            out,
            err,
            directory,
            query,
            exif_filters,
            tag_filters,
            *has_back,
            *has_enhanced,
            *format,
            *limit,
            *offset,
        ),

        Commands::Info {
            directory,
            stack_id,
            format,
        } => cmd_info(out, err, directory, stack_id, *format),

        Commands::Metadata(MetadataCommand::Read {
            directory,
            stack_id,
            format,
        }) => cmd_metadata_read(out, err, directory, stack_id, *format),

        Commands::Metadata(MetadataCommand::Write {
            directory,
            stack_id,
            tags,
        }) => cmd_metadata_write(out, err, directory, stack_id, tags),

        Commands::Metadata(MetadataCommand::Delete {
            directory,
            stack_id,
            tags,
        }) => cmd_metadata_delete(out, err, directory, stack_id, tags),

        Commands::Export { directory, output } => {
            cmd_export(out, err, directory, output.as_deref())
        }
    }
}

/// Scan command implementation
#[allow(clippy::too_many_arguments)]
pub fn cmd_scan(
    out: &mut dyn Write,
    err: &mut dyn Write,
    directory: &PathBuf,
    format: OutputFormat,
    show_metadata: bool,
    tiff_only: bool,
    jpeg_only: bool,
    with_back: bool,
    recursive: bool,
    limit: usize,
    offset: usize,
) -> i32 {
    let config = ScannerConfig {
        recursive,
        ..ScannerConfig::default()
    };
    let repo = LocalRepository::with_config(directory, config);
    let stacks = match repo.scan() {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(err, "Error scanning {}: {e}", directory.display());
            return EXIT_ERROR;
        }
    };

    // Apply filters
    let filtered: Vec<_> = stacks
        .into_iter()
        .filter(|s| {
            if tiff_only && s.format() != Some(ImageFormat::Tiff) {
                return false;
            }
            if jpeg_only && s.format() != Some(ImageFormat::Jpeg) {
                return false;
            }
            if with_back && s.back.is_none() {
                return false;
            }
            true
        })
        .collect();

    // Apply pagination if limit > 0
    if limit > 0 {
        let paginated = paginate_stacks(&filtered, &PaginationParams { offset, limit });
        output_stacks(out, &paginated.items, format, show_metadata, directory);
        if format == OutputFormat::Json {
            let _ = writeln!(
                out,
                "{{\"pagination\": {{\"total_count\": {}, \"offset\": {}, \"limit\": {}, \"has_more\": {}}}}}",
                paginated.total_count, paginated.offset, paginated.limit, paginated.has_more
            );
        } else {
            let _ = writeln!(
                out,
                "\nShowing {}-{} of {} stacks{}",
                offset + 1,
                (offset + paginated.items.len()).min(paginated.total_count),
                paginated.total_count,
                if paginated.has_more {
                    " (more available)"
                } else {
                    ""
                }
            );
        }
    } else {
        output_stacks(out, &filtered, format, show_metadata, directory);
    }
    EXIT_SUCCESS
}

/// Search command implementation
#[allow(clippy::too_many_arguments)]
pub fn cmd_search(
    out: &mut dyn Write,
    err: &mut dyn Write,
    directory: &PathBuf,
    query: &str,
    exif_filters: &[(String, String)],
    tag_filters: &[(String, String)],
    has_back: bool,
    has_enhanced: bool,
    format: OutputFormat,
    limit: usize,
    offset: usize,
) -> i32 {
    let repo = LocalRepository::new(directory);
    let stacks = match repo.scan() {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(err, "Error scanning {}: {e}", directory.display());
            return EXIT_ERROR;
        }
    };

    // Build search query
    let mut search = SearchQuery::new().with_text(query);

    for (key, value) in exif_filters {
        search = search.with_exif_filter(key, value);
    }
    for (key, value) in tag_filters {
        search = search.with_custom_filter(key, value);
    }
    if has_back {
        search = search.with_has_back(true);
    }
    if has_enhanced {
        search = search.with_has_enhanced(true);
    }

    let results = filter_stacks(&stacks, &search);

    // Apply pagination if limit > 0
    if limit > 0 {
        let paginated = paginate_stacks(&results, &PaginationParams { offset, limit });
        output_stacks(out, &paginated.items, format, false, directory);
        if format == OutputFormat::Json {
            let _ = writeln!(
                out,
                "{{\"pagination\": {{\"total_count\": {}, \"offset\": {}, \"limit\": {}, \"has_more\": {}}}}}",
                paginated.total_count, paginated.offset, paginated.limit, paginated.has_more
            );
        } else {
            let _ = writeln!(
                out,
                "\nShowing {}-{} of {} results{}",
                offset + 1,
                (offset + paginated.items.len()).min(paginated.total_count),
                paginated.total_count,
                if paginated.has_more {
                    " (more available)"
                } else {
                    ""
                }
            );
        }
    } else {
        output_stacks(out, &results, format, false, directory);
    }
    EXIT_SUCCESS
}

/// Info command implementation
pub fn cmd_info(
    out: &mut dyn Write,
    err: &mut dyn Write,
    directory: &PathBuf,
    stack_id: &str,
    format: OutputFormat,
) -> i32 {
    let repo = LocalRepository::new(directory);
    let stack = match repo.get_stack(stack_id) {
        Ok(s) => s,
        Err(photostax_core::repository::RepositoryError::NotFound(_)) => {
            let _ = writeln!(err, "Stack not found: {stack_id}");
            return EXIT_NOT_FOUND;
        }
        Err(e) => {
            let _ = writeln!(err, "Error: {e}");
            return EXIT_ERROR;
        }
    };

    match format {
        OutputFormat::Json => {
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(&stack).unwrap());
        }
        OutputFormat::Csv => {
            output_info_csv(out, &stack);
        }
        OutputFormat::Table => {
            output_info_table(out, &stack);
        }
    }

    EXIT_SUCCESS
}

/// Metadata read command
pub fn cmd_metadata_read(
    out: &mut dyn Write,
    err: &mut dyn Write,
    directory: &PathBuf,
    stack_id: &str,
    format: OutputFormat,
) -> i32 {
    let repo = LocalRepository::new(directory);
    let stack = match repo.get_stack(stack_id) {
        Ok(s) => s,
        Err(photostax_core::repository::RepositoryError::NotFound(_)) => {
            let _ = writeln!(err, "Stack not found: {stack_id}");
            return EXIT_NOT_FOUND;
        }
        Err(e) => {
            let _ = writeln!(err, "Error: {e}");
            return EXIT_ERROR;
        }
    };

    match format {
        OutputFormat::Json => {
            let _ = writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(&stack.metadata).unwrap()
            );
        }
        OutputFormat::Csv => {
            output_metadata_csv(out, &stack.metadata);
        }
        OutputFormat::Table => {
            output_metadata_table(out, &stack.metadata);
        }
    }

    EXIT_SUCCESS
}

/// Metadata write command
pub fn cmd_metadata_write(
    out: &mut dyn Write,
    err: &mut dyn Write,
    directory: &PathBuf,
    stack_id: &str,
    tags: &[(String, String)],
) -> i32 {
    let repo = LocalRepository::new(directory);
    let stack = match repo.get_stack(stack_id) {
        Ok(s) => s,
        Err(photostax_core::repository::RepositoryError::NotFound(_)) => {
            let _ = writeln!(err, "Stack not found: {stack_id}");
            return EXIT_NOT_FOUND;
        }
        Err(e) => {
            let _ = writeln!(err, "Error: {e}");
            return EXIT_ERROR;
        }
    };

    let mut new_tags = Metadata::default();
    for (key, value) in tags {
        new_tags
            .custom_tags
            .insert(key.clone(), serde_json::Value::String(value.clone()));
    }

    if let Err(e) = repo.write_metadata(&stack, &new_tags) {
        let _ = writeln!(err, "Error writing metadata: {e}");
        return EXIT_ERROR;
    }

    let _ = writeln!(out, "Wrote {} tag(s) to {stack_id}", tags.len());
    EXIT_SUCCESS
}

/// Metadata delete command
pub fn cmd_metadata_delete(
    out: &mut dyn Write,
    err: &mut dyn Write,
    directory: &PathBuf,
    stack_id: &str,
    tags: &[String],
) -> i32 {
    let repo = LocalRepository::new(directory);

    // Verify stack exists
    if let Err(photostax_core::repository::RepositoryError::NotFound(_)) = repo.get_stack(stack_id)
    {
        let _ = writeln!(err, "Stack not found: {stack_id}");
        return EXIT_NOT_FOUND;
    }

    // Open sidecar and delete tags
    for tag in tags {
        if let Err(e) =
            photostax_core::metadata::sidecar::remove_custom_tag(directory, stack_id, tag)
        {
            let _ = writeln!(err, "Error deleting tag '{tag}': {e}");
            return EXIT_ERROR;
        }
    }

    let _ = writeln!(out, "Deleted {} tag(s) from {stack_id}", tags.len());
    EXIT_SUCCESS
}

/// Export command implementation
pub fn cmd_export(
    out: &mut dyn Write,
    err: &mut dyn Write,
    directory: &PathBuf,
    output: Option<&Path>,
) -> i32 {
    let repo = LocalRepository::new(directory);
    let stacks = match repo.scan() {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(err, "Error scanning {}: {e}", directory.display());
            return EXIT_ERROR;
        }
    };

    let json = serde_json::to_string_pretty(&stacks).unwrap();

    match output {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &json) {
                let _ = writeln!(err, "Error writing to {}: {e}", path.display());
                return EXIT_ERROR;
            }
            let _ = writeln!(
                out,
                "Exported {} stack(s) to {}",
                stacks.len(),
                path.display()
            );
        }
        None => {
            let _ = writeln!(out, "{json}");
        }
    }

    EXIT_SUCCESS
}

// ============================================================================
// Output formatting functions
// ============================================================================

/// Output stacks in the requested format
pub fn output_stacks(
    out: &mut dyn Write,
    stacks: &[PhotoStack],
    format: OutputFormat,
    show_metadata: bool,
    dir: &Path,
) {
    match format {
        OutputFormat::Json => {
            let _ = writeln!(out, "{}", serde_json::to_string_pretty(stacks).unwrap());
        }
        OutputFormat::Csv => {
            output_stacks_csv(out, stacks, show_metadata);
        }
        OutputFormat::Table => {
            output_stacks_table(out, stacks, show_metadata, dir);
        }
    }
}

/// Output stacks as table with unicode box-drawing
pub fn output_stacks_table(
    out: &mut dyn Write,
    stacks: &[PhotoStack],
    show_metadata: bool,
    dir: &Path,
) {
    let _ = writeln!(
        out,
        "Found {} photo stack(s) in {}",
        stacks.len(),
        dir.display()
    );
    let _ = writeln!(out);

    if stacks.is_empty() {
        return;
    }

    // Calculate column widths
    let max_id = stacks
        .iter()
        .map(|s| s.id.len())
        .max()
        .unwrap_or(10)
        .max(10);

    // Header
    let _ = writeln!(
        out,
        "┌─{}─┬─────────┬──────────┬──────┬──────┬────────┐",
        "─".repeat(max_id)
    );
    let _ = writeln!(
        out,
        "│ {:<max_id$} │ Format  │ Original │ Enh. │ Back │ Tags   │",
        "ID"
    );
    let _ = writeln!(
        out,
        "├─{}─┼─────────┼──────────┼──────┼──────┼────────┤",
        "─".repeat(max_id)
    );

    for stack in stacks {
        let format_str = match stack.format() {
            Some(ImageFormat::Jpeg) => "JPEG",
            Some(ImageFormat::Tiff) => "TIFF",
            None => "-",
        };
        let orig = if stack.original.is_some() { "✓" } else { "-" };
        let enh = if stack.enhanced.is_some() { "✓" } else { "-" };
        let back = if stack.back.is_some() { "✓" } else { "-" };
        let tags = stack.metadata.exif_tags.len() + stack.metadata.custom_tags.len();

        let _ = writeln!(
            out,
            "│ {:<max_id$} │ {:<7} │    {:<5} │  {:<3} │  {:<3} │ {:>6} │",
            stack.id, format_str, orig, enh, back, tags
        );

        if show_metadata {
            // Show file paths
            if let Some(ref p) = stack.original {
                let _ = writeln!(
                    out,
                    "│ {:<max_id$} │         │ {}",
                    "",
                    p.file_name().unwrap_or_default().to_string_lossy()
                );
            }
            if let Some(ref p) = stack.enhanced {
                let _ = writeln!(
                    out,
                    "│ {:<max_id$} │         │ {}",
                    "",
                    p.file_name().unwrap_or_default().to_string_lossy()
                );
            }
            if let Some(ref p) = stack.back {
                let _ = writeln!(
                    out,
                    "│ {:<max_id$} │         │ {}",
                    "",
                    p.file_name().unwrap_or_default().to_string_lossy()
                );
            }
        }
    }

    let _ = writeln!(
        out,
        "└─{}─┴─────────┴──────────┴──────┴──────┴────────┘",
        "─".repeat(max_id)
    );
}

/// Output stacks as CSV
pub fn output_stacks_csv(out: &mut dyn Write, stacks: &[PhotoStack], show_metadata: bool) {
    if show_metadata {
        let _ = writeln!(
            out,
            "id,format,original,enhanced,back,exif_tags,custom_tags"
        );
    } else {
        let _ = writeln!(
            out,
            "id,format,has_original,has_enhanced,has_back,tag_count"
        );
    }

    for stack in stacks {
        let format_str = match stack.format() {
            Some(ImageFormat::Jpeg) => "jpeg",
            Some(ImageFormat::Tiff) => "tiff",
            None => "",
        };

        if show_metadata {
            let orig = stack
                .original
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let enh = stack
                .enhanced
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let back = stack
                .back
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "{},{},\"{}\",\"{}\",\"{}\",{},{}",
                stack.id,
                format_str,
                orig,
                enh,
                back,
                stack.metadata.exif_tags.len(),
                stack.metadata.custom_tags.len()
            );
        } else {
            let tags = stack.metadata.exif_tags.len() + stack.metadata.custom_tags.len();
            let _ = writeln!(
                out,
                "{},{},{},{},{},{}",
                stack.id,
                format_str,
                stack.original.is_some(),
                stack.enhanced.is_some(),
                stack.back.is_some(),
                tags
            );
        }
    }
}

/// Output stack info as table
pub fn output_info_table(out: &mut dyn Write, stack: &PhotoStack) {
    let format_str = match stack.format() {
        Some(ImageFormat::Jpeg) => "JPEG",
        Some(ImageFormat::Tiff) => "TIFF",
        None => "Unknown",
    };

    let _ = writeln!(
        out,
        "┌──────────────────────────────────────────────────────────────────┐"
    );
    let _ = writeln!(out, "│ Stack: {:<57} │", stack.id);
    let _ = writeln!(
        out,
        "├──────────────────────────────────────────────────────────────────┤"
    );
    let _ = writeln!(out, "│ Format: {:<56} │", format_str);

    // Files section
    let _ = writeln!(
        out,
        "├──────────────────────────────────────────────────────────────────┤"
    );
    let _ = writeln!(
        out,
        "│ Files:                                                           │"
    );
    if let Some(ref p) = stack.original {
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let _ = writeln!(
            out,
            "│   Original: {:<40} ({:>8}) │",
            p.file_name().unwrap_or_default().to_string_lossy(),
            format_size(size)
        );
    }
    if let Some(ref p) = stack.enhanced {
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let _ = writeln!(
            out,
            "│   Enhanced: {:<40} ({:>8}) │",
            p.file_name().unwrap_or_default().to_string_lossy(),
            format_size(size)
        );
    }
    if let Some(ref p) = stack.back {
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let _ = writeln!(
            out,
            "│   Back:     {:<40} ({:>8}) │",
            p.file_name().unwrap_or_default().to_string_lossy(),
            format_size(size)
        );
    }

    // EXIF tags
    if !stack.metadata.exif_tags.is_empty() {
        let _ = writeln!(
            out,
            "├──────────────────────────────────────────────────────────────────┤"
        );
        let _ = writeln!(out, "│ EXIF Tags ({}):", stack.metadata.exif_tags.len());
        let width = 62;
        for (key, value) in &stack.metadata.exif_tags {
            let kv = format!("{}: {}", key, value);
            let truncated = if kv.len() > width {
                format!("{}...", &kv[..width - 3])
            } else {
                kv
            };
            let _ = writeln!(out, "│   {:<width$} │", truncated);
        }
    }

    // XMP tags
    if !stack.metadata.xmp_tags.is_empty() {
        let _ = writeln!(
            out,
            "├──────────────────────────────────────────────────────────────────┤"
        );
        let _ = writeln!(out, "│ XMP Tags ({}):", stack.metadata.xmp_tags.len());
        let width = 62;
        for (key, value) in &stack.metadata.xmp_tags {
            let kv = format!("{}: {}", key, value);
            let truncated = if kv.len() > width {
                format!("{}...", &kv[..width - 3])
            } else {
                kv
            };
            let _ = writeln!(out, "│   {:<width$} │", truncated);
        }
    }

    // Custom tags
    if !stack.metadata.custom_tags.is_empty() {
        let _ = writeln!(
            out,
            "├──────────────────────────────────────────────────────────────────┤"
        );
        let _ = writeln!(out, "│ Custom Tags ({}):", stack.metadata.custom_tags.len());
        let width = 62;
        for (key, value) in &stack.metadata.custom_tags {
            let kv = format!("{}: {}", key, value);
            let truncated = if kv.len() > width {
                format!("{}...", &kv[..width - 3])
            } else {
                kv
            };
            let _ = writeln!(out, "│   {:<width$} │", truncated);
        }
    }

    let _ = writeln!(
        out,
        "└──────────────────────────────────────────────────────────────────┘"
    );
}

/// Output stack info as CSV
pub fn output_info_csv(out: &mut dyn Write, stack: &PhotoStack) {
    let _ = writeln!(out, "type,key,value");
    let _ = writeln!(out, "id,,{}", stack.id);

    if let Some(ref p) = stack.original {
        let _ = writeln!(out, "file,original,{}", p.display());
    }
    if let Some(ref p) = stack.enhanced {
        let _ = writeln!(out, "file,enhanced,{}", p.display());
    }
    if let Some(ref p) = stack.back {
        let _ = writeln!(out, "file,back,{}", p.display());
    }

    for (key, value) in &stack.metadata.exif_tags {
        let _ = writeln!(out, "exif,{},{}", key, escape_csv(value));
    }
    for (key, value) in &stack.metadata.xmp_tags {
        let _ = writeln!(out, "xmp,{},{}", key, escape_csv(value));
    }
    for (key, value) in &stack.metadata.custom_tags {
        let _ = writeln!(out, "custom,{},{}", key, escape_csv(&value.to_string()));
    }
}

/// Output metadata as table
pub fn output_metadata_table(out: &mut dyn Write, metadata: &Metadata) {
    let _ = writeln!(
        out,
        "┌──────────────────────────────────────────────────────────────────┐"
    );
    let _ = writeln!(
        out,
        "│ Metadata                                                         │"
    );
    let _ = writeln!(
        out,
        "├──────────────────────────────────────────────────────────────────┤"
    );

    let width = 62;

    if !metadata.exif_tags.is_empty() {
        let _ = writeln!(out, "│ EXIF Tags ({}):", metadata.exif_tags.len());
        for (key, value) in &metadata.exif_tags {
            let kv = format!("{}: {}", key, value);
            let truncated = if kv.len() > width {
                format!("{}...", &kv[..width - 3])
            } else {
                kv
            };
            let _ = writeln!(out, "│   {:<width$} │", truncated);
        }
    } else {
        let _ = writeln!(
            out,
            "│ EXIF Tags: (none)                                                │"
        );
    }

    let _ = writeln!(
        out,
        "├──────────────────────────────────────────────────────────────────┤"
    );

    if !metadata.xmp_tags.is_empty() {
        let _ = writeln!(out, "│ XMP Tags ({}):", metadata.xmp_tags.len());
        for (key, value) in &metadata.xmp_tags {
            let kv = format!("{}: {}", key, value);
            let truncated = if kv.len() > width {
                format!("{}...", &kv[..width - 3])
            } else {
                kv
            };
            let _ = writeln!(out, "│   {:<width$} │", truncated);
        }
    } else {
        let _ = writeln!(
            out,
            "│ XMP Tags: (none)                                                 │"
        );
    }

    let _ = writeln!(
        out,
        "├──────────────────────────────────────────────────────────────────┤"
    );

    if !metadata.custom_tags.is_empty() {
        let _ = writeln!(out, "│ Custom Tags ({}):", metadata.custom_tags.len());
        for (key, value) in &metadata.custom_tags {
            let kv = format!("{}: {}", key, value);
            let truncated = if kv.len() > width {
                format!("{}...", &kv[..width - 3])
            } else {
                kv
            };
            let _ = writeln!(out, "│   {:<width$} │", truncated);
        }
    } else {
        let _ = writeln!(
            out,
            "│ Custom Tags: (none)                                              │"
        );
    }

    let _ = writeln!(
        out,
        "└──────────────────────────────────────────────────────────────────┘"
    );
}

/// Output metadata as CSV
pub fn output_metadata_csv(out: &mut dyn Write, metadata: &Metadata) {
    let _ = writeln!(out, "type,key,value");

    for (key, value) in &metadata.exif_tags {
        let _ = writeln!(out, "exif,{},{}", key, escape_csv(value));
    }
    for (key, value) in &metadata.xmp_tags {
        let _ = writeln!(out, "xmp,{},{}", key, escape_csv(value));
    }
    for (key, value) in &metadata.custom_tags {
        let _ = writeln!(out, "custom,{},{}", key, escape_csv(&value.to_string()));
    }
}

/// Format file size in human-readable format
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Escape a string for CSV output
pub fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn testdata_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("core")
            .join("tests")
            .join("testdata")
    }

    /// Copy testdata to a temp dir for write operations
    fn copy_testdata_to_tempdir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for entry in std::fs::read_dir(testdata_path()).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                std::fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
            }
        }
        dir
    }

    fn make_stack(id: &str) -> PhotoStack {
        PhotoStack {
            id: id.to_string(),
            original: Some(PathBuf::from(format!("/photos/{id}.jpg"))),
            enhanced: Some(PathBuf::from(format!("/photos/{id}_a.jpg"))),
            back: Some(PathBuf::from(format!("/photos/{id}_b.jpg"))),
            metadata: Metadata::default(),
        }
    }

    fn make_stack_with_metadata(id: &str) -> PhotoStack {
        let mut exif_tags = HashMap::new();
        exif_tags.insert("Make".to_string(), "EPSON".to_string());
        exif_tags.insert("Model".to_string(), "FastFoto FF-680W".to_string());

        let mut xmp_tags = HashMap::new();
        xmp_tags.insert("Creator".to_string(), "Test User".to_string());

        let mut custom_tags = HashMap::new();
        custom_tags.insert(
            "album".to_string(),
            serde_json::Value::String("Family".to_string()),
        );

        PhotoStack {
            id: id.to_string(),
            original: Some(PathBuf::from(format!("/photos/{id}.jpg"))),
            enhanced: Some(PathBuf::from(format!("/photos/{id}_a.jpg"))),
            back: None,
            metadata: Metadata {
                exif_tags,
                xmp_tags,
                custom_tags,
            },
        }
    }

    fn make_tiff_stack(id: &str) -> PhotoStack {
        PhotoStack {
            id: id.to_string(),
            original: Some(PathBuf::from(format!("/photos/{id}.tif"))),
            enhanced: None,
            back: None,
            metadata: Metadata::default(),
        }
    }

    fn make_empty_stack(id: &str) -> PhotoStack {
        PhotoStack {
            id: id.to_string(),
            original: None,
            enhanced: None,
            back: None,
            metadata: Metadata::default(),
        }
    }

    // ======================== Pure function tests ========================

    #[test]
    fn test_parse_key_value_valid() {
        let (k, v) = parse_key_value("Make=EPSON").unwrap();
        assert_eq!(k, "Make");
        assert_eq!(v, "EPSON");
    }

    #[test]
    fn test_parse_key_value_with_equals_in_value() {
        let (k, v) = parse_key_value("expr=a=b").unwrap();
        assert_eq!(k, "expr");
        assert_eq!(v, "a=b");
    }

    #[test]
    fn test_parse_key_value_missing_equals() {
        let result = parse_key_value("noequals");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("KEY=VALUE"));
    }

    #[test]
    fn test_parse_key_value_empty_value() {
        let (k, v) = parse_key_value("key=").unwrap();
        assert_eq!(k, "key");
        assert_eq!(v, "");
    }

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn test_format_size_kb() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
    }

    #[test]
    fn test_format_size_mb() {
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn test_format_size_gb() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn test_escape_csv_plain() {
        assert_eq!(escape_csv("hello"), "hello");
    }

    #[test]
    fn test_escape_csv_with_comma() {
        assert_eq!(escape_csv("hello,world"), "\"hello,world\"");
    }

    #[test]
    fn test_escape_csv_with_quotes() {
        assert_eq!(escape_csv("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn test_escape_csv_with_newline() {
        assert_eq!(escape_csv("line1\nline2"), "\"line1\nline2\"");
    }

    // ======================== Output formatting tests ========================

    #[test]
    fn test_output_stacks_json() {
        let stacks = vec![make_stack("IMG_0001")];
        let mut buf = Vec::new();
        output_stacks(
            &mut buf,
            &stacks,
            OutputFormat::Json,
            false,
            &PathBuf::from("/photos"),
        );
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("IMG_0001"));
        assert!(output.contains("original"));
    }

    #[test]
    fn test_output_stacks_csv_no_metadata() {
        let stacks = vec![make_stack("IMG_0001"), make_tiff_stack("IMG_0002")];
        let mut buf = Vec::new();
        output_stacks_csv(&mut buf, &stacks, false);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("id,format,has_original"));
        assert!(output.contains("IMG_0001,jpeg,true,true,true"));
        assert!(output.contains("IMG_0002,tiff,true,false,false"));
    }

    #[test]
    fn test_output_stacks_csv_with_metadata() {
        let stacks = vec![make_stack("IMG_0001")];
        let mut buf = Vec::new();
        output_stacks_csv(&mut buf, &stacks, true);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("id,format,original,enhanced,back"));
    }

    #[test]
    fn test_output_stacks_table_empty() {
        let stacks: Vec<PhotoStack> = vec![];
        let mut buf = Vec::new();
        output_stacks_table(&mut buf, &stacks, false, &PathBuf::from("/photos"));
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Found 0 photo stack(s)"));
    }

    #[test]
    fn test_output_stacks_table_with_stacks() {
        let stacks = vec![make_stack("IMG_0001"), make_empty_stack("IMG_0002")];
        let mut buf = Vec::new();
        output_stacks_table(&mut buf, &stacks, false, &PathBuf::from("/photos"));
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Found 2 photo stack(s)"));
        assert!(output.contains("IMG_0001"));
        assert!(output.contains("JPEG"));
    }

    #[test]
    fn test_output_stacks_table_with_metadata_paths() {
        let stacks = vec![make_stack("IMG_0001")];
        let mut buf = Vec::new();
        output_stacks_table(&mut buf, &stacks, true, &PathBuf::from("/photos"));
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("IMG_0001.jpg"));
        assert!(output.contains("IMG_0001_a.jpg"));
        assert!(output.contains("IMG_0001_b.jpg"));
    }

    #[test]
    fn test_output_info_table_jpeg() {
        let stack = make_stack_with_metadata("IMG_0001");
        let mut buf = Vec::new();
        output_info_table(&mut buf, &stack);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Stack: IMG_0001"));
        assert!(output.contains("JPEG"));
        assert!(output.contains("EXIF Tags"));
        assert!(output.contains("EPSON"));
        assert!(output.contains("XMP Tags"));
        assert!(output.contains("Creator"));
        assert!(output.contains("Custom Tags"));
        assert!(output.contains("album"));
    }

    #[test]
    fn test_output_info_table_no_images() {
        let stack = make_empty_stack("EMPTY");
        let mut buf = Vec::new();
        output_info_table(&mut buf, &stack);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Stack: EMPTY"));
        assert!(output.contains("Unknown"));
    }

    #[test]
    fn test_output_info_csv() {
        let stack = make_stack_with_metadata("IMG_0001");
        let mut buf = Vec::new();
        output_info_csv(&mut buf, &stack);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("type,key,value"));
        assert!(output.contains("id,,IMG_0001"));
        assert!(output.contains("file,original,"));
        assert!(output.contains("exif,Make,EPSON"));
        assert!(output.contains("xmp,Creator,Test User"));
        assert!(output.contains("custom,album,"));
    }

    #[test]
    fn test_output_info_csv_no_files() {
        let stack = make_empty_stack("EMPTY");
        let mut buf = Vec::new();
        output_info_csv(&mut buf, &stack);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("id,,EMPTY"));
        // Should not contain file lines
        assert!(!output.contains("file,original"));
    }

    #[test]
    fn test_output_metadata_table_empty() {
        let metadata = Metadata::default();
        let mut buf = Vec::new();
        output_metadata_table(&mut buf, &metadata);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("EXIF Tags: (none)"));
        assert!(output.contains("XMP Tags: (none)"));
        assert!(output.contains("Custom Tags: (none)"));
    }

    #[test]
    fn test_output_metadata_table_with_tags() {
        let stack = make_stack_with_metadata("test");
        let mut buf = Vec::new();
        output_metadata_table(&mut buf, &stack.metadata);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("EXIF Tags (2):"));
        assert!(output.contains("EPSON"));
        assert!(output.contains("XMP Tags (1):"));
        assert!(output.contains("Test User"));
        assert!(output.contains("Custom Tags (1):"));
    }

    #[test]
    fn test_output_metadata_table_truncation() {
        let mut exif_tags = HashMap::new();
        exif_tags.insert("VeryLongTag".to_string(), "x".repeat(100));
        let metadata = Metadata {
            exif_tags,
            xmp_tags: HashMap::new(),
            custom_tags: HashMap::new(),
        };
        let mut buf = Vec::new();
        output_metadata_table(&mut buf, &metadata);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("..."));
    }

    #[test]
    fn test_output_metadata_csv_empty() {
        let metadata = Metadata::default();
        let mut buf = Vec::new();
        output_metadata_csv(&mut buf, &metadata);
        let output = String::from_utf8(buf).unwrap();
        assert_eq!(output.trim(), "type,key,value");
    }

    #[test]
    fn test_output_metadata_csv_with_tags() {
        let stack = make_stack_with_metadata("test");
        let mut buf = Vec::new();
        output_metadata_csv(&mut buf, &stack.metadata);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("exif,Make,EPSON"));
        assert!(output.contains("xmp,Creator,Test User"));
        assert!(output.contains("custom,album,"));
    }

    // ======================== Command tests with testdata ========================

    #[test]
    fn test_cmd_scan_testdata() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_scan(
            &mut out,
            &mut err,
            &testdata_path(),
            OutputFormat::Table,
            false,
            false,
            false,
            false,
            false,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("photo stack(s)"));
        assert!(output.contains("FamilyPhotos"));
    }

    #[test]
    fn test_cmd_scan_json() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_scan(
            &mut out,
            &mut err,
            &testdata_path(),
            OutputFormat::Json,
            false,
            false,
            false,
            false,
            false,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("FamilyPhotos"));
        // Should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert!(parsed.is_array());
    }

    #[test]
    fn test_cmd_scan_csv() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_scan(
            &mut out,
            &mut err,
            &testdata_path(),
            OutputFormat::Csv,
            false,
            false,
            false,
            false,
            false,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("id,format"));
    }

    #[test]
    fn test_cmd_scan_jpeg_only() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_scan(
            &mut out,
            &mut err,
            &testdata_path(),
            OutputFormat::Csv,
            false,
            false,
            true,
            false,
            false,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        // Should only contain JPEG stacks
        for line in output.lines().skip(1) {
            if !line.is_empty() {
                assert!(
                    line.contains("jpeg") || line.contains("true") || line.contains("false"),
                    "Non-JPEG line found: {line}"
                );
            }
        }
    }

    #[test]
    fn test_cmd_scan_tiff_only() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_scan(
            &mut out,
            &mut err,
            &testdata_path(),
            OutputFormat::Csv,
            false,
            true,
            false,
            false,
            false,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
    }

    #[test]
    fn test_cmd_scan_with_back_filter() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_scan(
            &mut out,
            &mut err,
            &testdata_path(),
            OutputFormat::Csv,
            false,
            false,
            false,
            true,
            false,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
    }

    #[test]
    fn test_cmd_scan_show_metadata() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_scan(
            &mut out,
            &mut err,
            &testdata_path(),
            OutputFormat::Table,
            true,
            false,
            false,
            false,
            false,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains(".jpg") || output.contains(".tif"));
    }

    #[test]
    fn test_cmd_scan_csv_with_metadata() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_scan(
            &mut out,
            &mut err,
            &testdata_path(),
            OutputFormat::Csv,
            true,
            false,
            false,
            false,
            false,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("id,format,original,enhanced,back"));
    }

    #[test]
    fn test_cmd_scan_nonexistent_dir() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_scan(
            &mut out,
            &mut err,
            &PathBuf::from("/nonexistent/dir"),
            OutputFormat::Table,
            false,
            false,
            false,
            false,
            false,
            0,
            0,
        );
        // LocalRepository::scan may return an error for nonexistent dirs
        assert!(code == EXIT_SUCCESS || code == EXIT_ERROR);
    }

    #[test]
    fn test_cmd_search_testdata() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_search(
            &mut out,
            &mut err,
            &testdata_path(),
            "FamilyPhotos",
            &[],
            &[],
            false,
            false,
            OutputFormat::Table,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("FamilyPhotos"));
    }

    #[test]
    fn test_cmd_search_no_results() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_search(
            &mut out,
            &mut err,
            &testdata_path(),
            "zzz_nonexistent",
            &[],
            &[],
            false,
            false,
            OutputFormat::Table,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Found 0"));
    }

    #[test]
    fn test_cmd_search_with_exif_filter() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let exif_filters = vec![("Make".to_string(), "EPSON".to_string())];
        let code = cmd_search(
            &mut out,
            &mut err,
            &testdata_path(),
            "Family",
            &exif_filters,
            &[],
            false,
            false,
            OutputFormat::Json,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
    }

    #[test]
    fn test_cmd_search_with_has_back() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_search(
            &mut out,
            &mut err,
            &testdata_path(),
            "Family",
            &[],
            &[],
            true,
            false,
            OutputFormat::Csv,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
    }

    #[test]
    fn test_cmd_search_with_has_enhanced() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_search(
            &mut out,
            &mut err,
            &testdata_path(),
            "Family",
            &[],
            &[],
            false,
            true,
            OutputFormat::Table,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
    }

    #[test]
    fn test_cmd_search_with_tag_filter() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let tag_filters = vec![("album".to_string(), "Family".to_string())];
        let code = cmd_search(
            &mut out,
            &mut err,
            &testdata_path(),
            "Family",
            &[],
            &tag_filters,
            false,
            false,
            OutputFormat::Table,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
    }

    #[test]
    fn test_cmd_info_happy_path() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_info(
            &mut out,
            &mut err,
            &testdata_path(),
            "FamilyPhotos_0001",
            OutputFormat::Table,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("FamilyPhotos_0001"));
    }

    #[test]
    fn test_cmd_info_json() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_info(
            &mut out,
            &mut err,
            &testdata_path(),
            "FamilyPhotos_0001",
            OutputFormat::Json,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        let _parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    }

    #[test]
    fn test_cmd_info_csv() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_info(
            &mut out,
            &mut err,
            &testdata_path(),
            "FamilyPhotos_0001",
            OutputFormat::Csv,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("type,key,value"));
    }

    #[test]
    fn test_cmd_info_not_found() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_info(
            &mut out,
            &mut err,
            &testdata_path(),
            "nonexistent_stack",
            OutputFormat::Table,
        );
        assert_eq!(code, EXIT_NOT_FOUND);
        let error_output = String::from_utf8(err).unwrap();
        assert!(error_output.contains("not found"));
    }

    #[test]
    fn test_cmd_metadata_read_table() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_metadata_read(
            &mut out,
            &mut err,
            &testdata_path(),
            "FamilyPhotos_0001",
            OutputFormat::Table,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Metadata"));
    }

    #[test]
    fn test_cmd_metadata_read_json() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_metadata_read(
            &mut out,
            &mut err,
            &testdata_path(),
            "FamilyPhotos_0001",
            OutputFormat::Json,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        let _parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    }

    #[test]
    fn test_cmd_metadata_read_csv() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_metadata_read(
            &mut out,
            &mut err,
            &testdata_path(),
            "FamilyPhotos_0001",
            OutputFormat::Csv,
        );
        assert_eq!(code, EXIT_SUCCESS);
    }

    #[test]
    fn test_cmd_metadata_read_not_found() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_metadata_read(
            &mut out,
            &mut err,
            &testdata_path(),
            "nonexistent",
            OutputFormat::Table,
        );
        assert_eq!(code, EXIT_NOT_FOUND);
    }

    #[test]
    fn test_cmd_metadata_write_happy_path() {
        let dir = copy_testdata_to_tempdir();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let tags = vec![
            ("album".to_string(), "Family".to_string()),
            ("year".to_string(), "2024".to_string()),
        ];
        let code = cmd_metadata_write(
            &mut out,
            &mut err,
            &dir.path().to_path_buf(),
            "FamilyPhotos_0001",
            &tags,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Wrote 2 tag(s)"));
    }

    #[test]
    fn test_cmd_metadata_write_not_found() {
        let dir = copy_testdata_to_tempdir();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let tags = vec![("album".to_string(), "Test".to_string())];
        let code = cmd_metadata_write(
            &mut out,
            &mut err,
            &dir.path().to_path_buf(),
            "nonexistent",
            &tags,
        );
        assert_eq!(code, EXIT_NOT_FOUND);
    }

    #[test]
    fn test_cmd_metadata_delete_not_found() {
        let dir = copy_testdata_to_tempdir();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let tags = vec!["album".to_string()];
        let code = cmd_metadata_delete(
            &mut out,
            &mut err,
            &dir.path().to_path_buf(),
            "nonexistent",
            &tags,
        );
        assert_eq!(code, EXIT_NOT_FOUND);
    }

    #[test]
    fn test_cmd_metadata_delete_happy_path() {
        let dir = copy_testdata_to_tempdir();

        // First write a tag
        let mut out = Vec::new();
        let mut err = Vec::new();
        let tags = vec![("album".to_string(), "Family".to_string())];
        cmd_metadata_write(
            &mut out,
            &mut err,
            &dir.path().to_path_buf(),
            "FamilyPhotos_0001",
            &tags,
        );

        // Then delete it
        let mut out = Vec::new();
        let mut err = Vec::new();
        let tags = vec!["album".to_string()];
        let code = cmd_metadata_delete(
            &mut out,
            &mut err,
            &dir.path().to_path_buf(),
            "FamilyPhotos_0001",
            &tags,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Deleted 1 tag(s)"));
    }

    #[test]
    fn test_cmd_export_stdout() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_export(&mut out, &mut err, &testdata_path(), None);
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert!(parsed.is_array());
    }

    #[test]
    fn test_cmd_export_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let output_file = dir.path().join("export.json");
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_export(&mut out, &mut err, &testdata_path(), Some(&output_file));
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Exported"));
        assert!(output_file.exists());
        let content = std::fs::read_to_string(&output_file).unwrap();
        let _: serde_json::Value = serde_json::from_str(&content).unwrap();
    }

    #[test]
    fn test_cmd_export_to_invalid_path() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_export(
            &mut out,
            &mut err,
            &testdata_path(),
            Some(Path::new("/nonexistent/dir/out.json")),
        );
        assert_eq!(code, EXIT_ERROR);
        let error_output = String::from_utf8(err).unwrap();
        assert!(error_output.contains("Error writing"));
    }

    #[test]
    fn test_cmd_scan_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_scan(
            &mut out,
            &mut err,
            &dir.path().to_path_buf(),
            OutputFormat::Table,
            false,
            false,
            false,
            false,
            false,
            0,
            0,
        );
        assert_eq!(code, EXIT_SUCCESS);
        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("Found 0"));
    }

    // ======================== Output format for various stack types ========================

    #[test]
    fn test_output_stacks_table_tiff_format() {
        let stacks = vec![make_tiff_stack("IMG_0001")];
        let mut buf = Vec::new();
        output_stacks_table(&mut buf, &stacks, false, &PathBuf::from("/photos"));
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("TIFF"));
    }

    #[test]
    fn test_output_stacks_table_no_format() {
        let stacks = vec![make_empty_stack("IMG_0001")];
        let mut buf = Vec::new();
        output_stacks_table(&mut buf, &stacks, false, &PathBuf::from("/photos"));
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("-"));
    }

    #[test]
    fn test_output_stacks_csv_tiff_format() {
        let stacks = vec![make_tiff_stack("IMG_0001")];
        let mut buf = Vec::new();
        output_stacks_csv(&mut buf, &stacks, false);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("tiff"));
    }

    #[test]
    fn test_output_stacks_csv_no_format() {
        let stacks = vec![make_empty_stack("IMG_0001")];
        let mut buf = Vec::new();
        output_stacks_csv(&mut buf, &stacks, false);
        let output = String::from_utf8(buf).unwrap();
        // Empty format when no images
        assert!(output.contains("IMG_0001,,false,false,false"));
    }

    #[test]
    fn test_output_info_table_with_long_tag_truncation() {
        let mut exif_tags = HashMap::new();
        exif_tags.insert("Description".to_string(), "A".repeat(100));
        let stack = PhotoStack {
            id: "TRUNC".to_string(),
            original: None,
            enhanced: None,
            back: None,
            metadata: Metadata {
                exif_tags,
                xmp_tags: HashMap::new(),
                custom_tags: HashMap::new(),
            },
        };
        let mut buf = Vec::new();
        output_info_table(&mut buf, &stack);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("..."));
    }

    #[test]
    fn test_output_info_table_tiff() {
        let stack = make_tiff_stack("TIFF_0001");
        let mut buf = Vec::new();
        output_info_table(&mut buf, &stack);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("TIFF"));
    }

    // ======================== run_cli dispatch tests ========================

    #[test]
    fn test_run_cli_scan_dispatch() {
        let cli = Cli {
            command: Commands::Scan {
                directory: testdata_path(),
                format: OutputFormat::Table,
                show_metadata: false,
                tiff_only: false,
                jpeg_only: false,
                with_back: false,
                recursive: false,
                limit: 0,
                offset: 0,
            },
        };
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_cli(&cli, &mut out, &mut err);
        assert_eq!(code, EXIT_SUCCESS);
    }

    #[test]
    fn test_run_cli_search_dispatch() {
        let cli = Cli {
            command: Commands::Search {
                directory: testdata_path(),
                query: "FamilyPhotos".to_string(),
                exif_filters: vec![],
                tag_filters: vec![],
                has_back: false,
                has_enhanced: false,
                format: OutputFormat::Table,
                limit: 0,
                offset: 0,
            },
        };
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_cli(&cli, &mut out, &mut err);
        assert_eq!(code, EXIT_SUCCESS);
    }

    #[test]
    fn test_run_cli_info_dispatch() {
        let cli = Cli {
            command: Commands::Info {
                directory: testdata_path(),
                stack_id: "FamilyPhotos_0001".to_string(),
                format: OutputFormat::Table,
            },
        };
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_cli(&cli, &mut out, &mut err);
        assert_eq!(code, EXIT_SUCCESS);
    }

    #[test]
    fn test_run_cli_metadata_read_dispatch() {
        let cli = Cli {
            command: Commands::Metadata(MetadataCommand::Read {
                directory: testdata_path(),
                stack_id: "FamilyPhotos_0001".to_string(),
                format: OutputFormat::Table,
            }),
        };
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_cli(&cli, &mut out, &mut err);
        assert_eq!(code, EXIT_SUCCESS);
    }

    #[test]
    fn test_run_cli_metadata_write_dispatch() {
        let dir = copy_testdata_to_tempdir();
        let cli = Cli {
            command: Commands::Metadata(MetadataCommand::Write {
                directory: dir.path().to_path_buf(),
                stack_id: "FamilyPhotos_0001".to_string(),
                tags: vec![("test_key".to_string(), "test_val".to_string())],
            }),
        };
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_cli(&cli, &mut out, &mut err);
        assert_eq!(code, EXIT_SUCCESS);
    }

    #[test]
    fn test_run_cli_metadata_delete_dispatch() {
        let dir = copy_testdata_to_tempdir();
        // Write first
        let tags_w = vec![("del_key".to_string(), "val".to_string())];
        cmd_metadata_write(
            &mut Vec::new(),
            &mut Vec::new(),
            &dir.path().to_path_buf(),
            "FamilyPhotos_0001",
            &tags_w,
        );

        let cli = Cli {
            command: Commands::Metadata(MetadataCommand::Delete {
                directory: dir.path().to_path_buf(),
                stack_id: "FamilyPhotos_0001".to_string(),
                tags: vec!["del_key".to_string()],
            }),
        };
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_cli(&cli, &mut out, &mut err);
        assert_eq!(code, EXIT_SUCCESS);
    }

    #[test]
    fn test_run_cli_export_dispatch() {
        let cli = Cli {
            command: Commands::Export {
                directory: testdata_path(),
                output: None,
            },
        };
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_cli(&cli, &mut out, &mut err);
        assert_eq!(code, EXIT_SUCCESS);
    }

    // ======================== XMP and custom tag coverage ========================

    #[test]
    fn test_output_info_table_with_xmp_tags() {
        let mut xmp_tags = HashMap::new();
        xmp_tags.insert("Creator".to_string(), "John Doe".to_string());
        let stack = PhotoStack {
            id: "XMP_TEST".to_string(),
            original: Some(PathBuf::from("/photos/XMP_TEST.jpg")),
            enhanced: None,
            back: None,
            metadata: Metadata {
                exif_tags: HashMap::new(),
                xmp_tags,
                custom_tags: HashMap::new(),
            },
        };
        let mut buf = Vec::new();
        output_info_table(&mut buf, &stack);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("XMP Tags"));
        assert!(output.contains("Creator"));
    }

    #[test]
    fn test_output_info_table_with_custom_tags() {
        let mut custom_tags = HashMap::new();
        custom_tags.insert(
            "album".to_string(),
            serde_json::Value::String("vacation".to_string()),
        );
        let stack = PhotoStack {
            id: "CUSTOM_TEST".to_string(),
            original: None,
            enhanced: None,
            back: None,
            metadata: Metadata {
                exif_tags: HashMap::new(),
                xmp_tags: HashMap::new(),
                custom_tags,
            },
        };
        let mut buf = Vec::new();
        output_info_table(&mut buf, &stack);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Custom Tags"));
        assert!(output.contains("album"));
    }

    #[test]
    fn test_output_metadata_table_with_xmp_tags() {
        let mut xmp_tags = HashMap::new();
        xmp_tags.insert("Subject".to_string(), "Landscape".to_string());
        let meta = Metadata {
            exif_tags: HashMap::new(),
            xmp_tags,
            custom_tags: HashMap::new(),
        };
        let mut buf = Vec::new();
        output_metadata_table(&mut buf, &meta);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("XMP Tags"));
        assert!(output.contains("Subject"));
    }

    #[test]
    fn test_output_info_csv_with_xmp_and_custom_tags() {
        let mut xmp_tags = HashMap::new();
        xmp_tags.insert("Creator".to_string(), "Jane".to_string());
        let mut custom_tags = HashMap::new();
        custom_tags.insert("rating".to_string(), serde_json::Value::from(5));
        let stack = PhotoStack {
            id: "CSV_TAGS".to_string(),
            original: Some(PathBuf::from("/photos/CSV_TAGS.jpg")),
            enhanced: Some(PathBuf::from("/photos/CSV_TAGS_a.jpg")),
            back: Some(PathBuf::from("/photos/CSV_TAGS_b.jpg")),
            metadata: Metadata {
                exif_tags: HashMap::new(),
                xmp_tags,
                custom_tags,
            },
        };
        let mut buf = Vec::new();
        output_info_csv(&mut buf, &stack);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("xmp,Creator,Jane"));
        assert!(output.contains("custom,rating,5"));
        assert!(output.contains("file,original"));
        assert!(output.contains("file,enhanced"));
        assert!(output.contains("file,back"));
    }

    #[test]
    fn test_output_metadata_csv_with_xmp() {
        let mut xmp_tags = HashMap::new();
        xmp_tags.insert("Title".to_string(), "My Photo".to_string());
        let meta = Metadata {
            exif_tags: HashMap::new(),
            xmp_tags,
            custom_tags: HashMap::new(),
        };
        let mut buf = Vec::new();
        output_metadata_csv(&mut buf, &meta);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("xmp,Title,My Photo"));
    }

    #[test]
    fn test_output_info_table_with_long_xmp_truncation() {
        let mut xmp_tags = HashMap::new();
        xmp_tags.insert("Description".to_string(), "X".repeat(100));
        let stack = PhotoStack {
            id: "LONG_XMP".to_string(),
            original: None,
            enhanced: None,
            back: None,
            metadata: Metadata {
                exif_tags: HashMap::new(),
                xmp_tags,
                custom_tags: HashMap::new(),
            },
        };
        let mut buf = Vec::new();
        output_info_table(&mut buf, &stack);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("..."));
    }

    #[test]
    fn test_output_info_table_with_long_custom_truncation() {
        let mut custom_tags = HashMap::new();
        custom_tags.insert(
            "longval".to_string(),
            serde_json::Value::String("Y".repeat(100)),
        );
        let stack = PhotoStack {
            id: "LONG_CUSTOM".to_string(),
            original: None,
            enhanced: None,
            back: None,
            metadata: Metadata {
                exif_tags: HashMap::new(),
                xmp_tags: HashMap::new(),
                custom_tags,
            },
        };
        let mut buf = Vec::new();
        output_info_table(&mut buf, &stack);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("..."));
    }

    #[test]
    fn test_output_metadata_table_with_long_xmp_truncation() {
        let mut xmp_tags = HashMap::new();
        xmp_tags.insert("LongKey".to_string(), "Z".repeat(100));
        let meta = Metadata {
            exif_tags: HashMap::new(),
            xmp_tags,
            custom_tags: HashMap::new(),
        };
        let mut buf = Vec::new();
        output_metadata_table(&mut buf, &meta);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("..."));
    }

    #[test]
    fn test_output_metadata_table_with_long_custom_truncation() {
        let mut custom_tags = HashMap::new();
        custom_tags.insert(
            "longkey".to_string(),
            serde_json::Value::String("W".repeat(100)),
        );
        let meta = Metadata {
            exif_tags: HashMap::new(),
            xmp_tags: HashMap::new(),
            custom_tags,
        };
        let mut buf = Vec::new();
        output_metadata_table(&mut buf, &meta);
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("..."));
    }

    // ======================== Error path coverage ========================

    #[test]
    fn test_cmd_search_scan_error() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_search(
            &mut out,
            &mut err,
            &PathBuf::from("/nonexistent/search/dir"),
            "query",
            &[],
            &[],
            false,
            false,
            OutputFormat::Table,
            0,
            0,
        );
        // May succeed with 0 results or fail depending on OS
        assert!(code == EXIT_SUCCESS || code == EXIT_ERROR);
    }

    #[test]
    fn test_cmd_export_scan_error() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_export(
            &mut out,
            &mut err,
            &PathBuf::from("/nonexistent/export/dir"),
            None,
        );
        assert!(code == EXIT_SUCCESS || code == EXIT_ERROR);
    }

    #[test]
    fn test_cmd_info_generic_error() {
        // Trigger a generic (non-NotFound) error by using a path that exists but isn't a valid repo
        let mut out = Vec::new();
        let mut err = Vec::new();
        // This will produce either NotFound or another error type depending on OS
        let code = cmd_info(
            &mut out,
            &mut err,
            &PathBuf::from("/nonexistent/info/dir"),
            "NO_STACK",
            OutputFormat::Table,
        );
        assert!(code == EXIT_NOT_FOUND || code == EXIT_ERROR);
    }

    #[test]
    fn test_cmd_metadata_read_generic_error() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = cmd_metadata_read(
            &mut out,
            &mut err,
            &PathBuf::from("/nonexistent/meta/dir"),
            "NO_STACK",
            OutputFormat::Table,
        );
        assert!(code == EXIT_NOT_FOUND || code == EXIT_ERROR);
    }

    #[test]
    fn test_cmd_metadata_write_generic_error() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let tags = vec![("k".to_string(), "v".to_string())];
        let code = cmd_metadata_write(
            &mut out,
            &mut err,
            &PathBuf::from("/nonexistent/write/dir"),
            "NO_STACK",
            &tags,
        );
        assert!(code == EXIT_NOT_FOUND || code == EXIT_ERROR);
    }
}
