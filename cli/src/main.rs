//! # photostax-cli
//!
//! Command-line tool for inspecting and managing Epson FastFoto photo stacks.
//!
//! ## Usage
//!
//! ```text
//! photostax-cli <SUBCOMMAND>
//!
//! SUBCOMMANDS:
//!     scan       Scan a directory and list all photo stacks
//!     search     Search photo stacks by metadata
//!     info       Show detailed information about a specific stack
//!     metadata   Read or write metadata for a stack
//!     export     Export stack data as JSON
//!     help       Print help information
//! ```
//!
//! ## Examples
//!
//! List all photo stacks in a directory:
//!
//! ```text
//! photostax-cli scan /photos
//! ```
//!
//! Search for stacks containing "birthday":
//!
//! ```text
//! photostax-cli search /photos birthday
//! ```
//!
//! Show details about a specific stack:
//!
//! ```text
//! photostax-cli info /photos IMG_0001
//! ```
//!
//! Export all stacks as JSON:
//!
//! ```text
//! photostax-cli export /photos --output stacks.json
//! ```

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use photostax_core::backends::local::LocalRepository;
use photostax_core::metadata::ImageFormat;
use photostax_core::photo_stack::{Metadata, PhotoStack};
use photostax_core::repository::Repository;
use photostax_core::search::{filter_stacks, SearchQuery};

/// CLI tool for inspecting and managing Epson FastFoto photo stacks
#[derive(Parser)]
#[command(name = "photostax-cli")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan a directory and list all photo stacks
    #[command(long_about = "Scan a directory for Epson FastFoto photo stacks and display them.\n\n\
        FastFoto creates files with naming convention:\n  \
        - <name>.jpg/.tif      Original front scan\n  \
        - <name>_a.jpg/.tif    Enhanced (color-corrected)\n  \
        - <name>_b.jpg/.tif    Back of photo\n\n\
        These are grouped into 'stacks' for unified management.")]
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
    },

    /// Search photo stacks by metadata
    #[command(long_about = "Search photo stacks by text query and metadata filters.\n\n\
        The text query searches across stack IDs and all metadata values.\n\
        Additional filters can narrow results to specific EXIF or custom tags.")]
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
    },

    /// Show detailed information about a specific stack
    #[command(long_about = "Display comprehensive information about a single photo stack.\n\n\
        Shows all file paths, file sizes, and complete metadata including\n\
        EXIF tags, XMP tags, and custom tags from the sidecar database.")]
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
enum MetadataCommand {
    /// Read metadata for a stack
    #[command(long_about = "Display all metadata for a photo stack including EXIF, XMP, and custom tags.")]
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
        Tags are stored in the sidecar database (.photostax.db) and do not modify\n\
        the original image files.")]
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
    #[command(long_about = "Remove custom tags from a photo stack's sidecar database.")]
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
enum OutputFormat {
    /// Human-readable table with unicode box-drawing
    Table,
    /// JSON output
    Json,
    /// Comma-separated values
    Csv,
}

/// Parse KEY=VALUE format for tag filters
fn parse_key_value(s: &str) -> Result<(String, String), String> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid format '{s}', expected KEY=VALUE"));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

fn main() {
    let cli = Cli::parse();
    let exit_code = match cli.command {
        Commands::Scan {
            directory,
            format,
            show_metadata,
            tiff_only,
            jpeg_only,
            with_back,
        } => cmd_scan(&directory, format, show_metadata, tiff_only, jpeg_only, with_back),

        Commands::Search {
            directory,
            query,
            exif_filters,
            tag_filters,
            has_back,
            has_enhanced,
            format,
        } => cmd_search(
            &directory,
            &query,
            &exif_filters,
            &tag_filters,
            has_back,
            has_enhanced,
            format,
        ),

        Commands::Info {
            directory,
            stack_id,
            format,
        } => cmd_info(&directory, &stack_id, format),

        Commands::Metadata(MetadataCommand::Read {
            directory,
            stack_id,
            format,
        }) => cmd_metadata_read(&directory, &stack_id, format),

        Commands::Metadata(MetadataCommand::Write {
            directory,
            stack_id,
            tags,
        }) => cmd_metadata_write(&directory, &stack_id, &tags),

        Commands::Metadata(MetadataCommand::Delete {
            directory,
            stack_id,
            tags,
        }) => cmd_metadata_delete(&directory, &stack_id, &tags),

        Commands::Export { directory, output } => cmd_export(&directory, output.as_deref()),
    };

    std::process::exit(exit_code);
}

// Exit codes
const EXIT_SUCCESS: i32 = 0;
const EXIT_ERROR: i32 = 1;
const EXIT_NOT_FOUND: i32 = 2;

/// Scan command implementation
fn cmd_scan(
    directory: &PathBuf,
    format: OutputFormat,
    show_metadata: bool,
    tiff_only: bool,
    jpeg_only: bool,
    with_back: bool,
) -> i32 {
    let repo = LocalRepository::new(directory);
    let stacks = match repo.scan() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error scanning {}: {e}", directory.display());
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

    output_stacks(&filtered, format, show_metadata, directory);
    EXIT_SUCCESS
}

/// Search command implementation
fn cmd_search(
    directory: &PathBuf,
    query: &str,
    exif_filters: &[(String, String)],
    tag_filters: &[(String, String)],
    has_back: bool,
    has_enhanced: bool,
    format: OutputFormat,
) -> i32 {
    let repo = LocalRepository::new(directory);
    let stacks = match repo.scan() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error scanning {}: {e}", directory.display());
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
    output_stacks(&results, format, false, directory);
    EXIT_SUCCESS
}

/// Info command implementation
fn cmd_info(directory: &PathBuf, stack_id: &str, format: OutputFormat) -> i32 {
    let repo = LocalRepository::new(directory);
    let stack = match repo.get_stack(stack_id) {
        Ok(s) => s,
        Err(photostax_core::repository::RepositoryError::NotFound(_)) => {
            eprintln!("Stack not found: {stack_id}");
            return EXIT_NOT_FOUND;
        }
        Err(e) => {
            eprintln!("Error: {e}");
            return EXIT_ERROR;
        }
    };

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&stack).unwrap());
        }
        OutputFormat::Csv => {
            output_info_csv(&stack);
        }
        OutputFormat::Table => {
            output_info_table(&stack);
        }
    }

    EXIT_SUCCESS
}

/// Metadata read command
fn cmd_metadata_read(directory: &PathBuf, stack_id: &str, format: OutputFormat) -> i32 {
    let repo = LocalRepository::new(directory);
    let stack = match repo.get_stack(stack_id) {
        Ok(s) => s,
        Err(photostax_core::repository::RepositoryError::NotFound(_)) => {
            eprintln!("Stack not found: {stack_id}");
            return EXIT_NOT_FOUND;
        }
        Err(e) => {
            eprintln!("Error: {e}");
            return EXIT_ERROR;
        }
    };

    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&stack.metadata).unwrap());
        }
        OutputFormat::Csv => {
            output_metadata_csv(&stack.metadata);
        }
        OutputFormat::Table => {
            output_metadata_table(&stack.metadata);
        }
    }

    EXIT_SUCCESS
}

/// Metadata write command
fn cmd_metadata_write(directory: &PathBuf, stack_id: &str, tags: &[(String, String)]) -> i32 {
    let repo = LocalRepository::new(directory);
    let stack = match repo.get_stack(stack_id) {
        Ok(s) => s,
        Err(photostax_core::repository::RepositoryError::NotFound(_)) => {
            eprintln!("Stack not found: {stack_id}");
            return EXIT_NOT_FOUND;
        }
        Err(e) => {
            eprintln!("Error: {e}");
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
        eprintln!("Error writing metadata: {e}");
        return EXIT_ERROR;
    }

    println!("Wrote {} tag(s) to {stack_id}", tags.len());
    EXIT_SUCCESS
}

/// Metadata delete command
fn cmd_metadata_delete(directory: &PathBuf, stack_id: &str, tags: &[String]) -> i32 {
    let repo = LocalRepository::new(directory);

    // Verify stack exists
    if let Err(photostax_core::repository::RepositoryError::NotFound(_)) = repo.get_stack(stack_id)
    {
        eprintln!("Stack not found: {stack_id}");
        return EXIT_NOT_FOUND;
    }

    // Open sidecar DB and delete tags
    let db = match photostax_core::metadata::sidecar::SidecarDb::open(directory) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Error opening database: {e}");
            return EXIT_ERROR;
        }
    };

    for tag in tags {
        if let Err(e) = db.remove_tag(stack_id, tag) {
            eprintln!("Error deleting tag '{tag}': {e}");
            return EXIT_ERROR;
        }
    }

    println!("Deleted {} tag(s) from {stack_id}", tags.len());
    EXIT_SUCCESS
}

/// Export command implementation
fn cmd_export(directory: &PathBuf, output: Option<&std::path::Path>) -> i32 {
    let repo = LocalRepository::new(directory);
    let stacks = match repo.scan() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error scanning {}: {e}", directory.display());
            return EXIT_ERROR;
        }
    };

    let json = serde_json::to_string_pretty(&stacks).unwrap();

    match output {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &json) {
                eprintln!("Error writing to {}: {e}", path.display());
                return EXIT_ERROR;
            }
            println!("Exported {} stack(s) to {}", stacks.len(), path.display());
        }
        None => {
            println!("{json}");
        }
    }

    EXIT_SUCCESS
}

// ============================================================================
// Output formatting functions
// ============================================================================

/// Output stacks in the requested format
fn output_stacks(stacks: &[PhotoStack], format: OutputFormat, show_metadata: bool, dir: &PathBuf) {
    match format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(stacks).unwrap());
        }
        OutputFormat::Csv => {
            output_stacks_csv(stacks, show_metadata);
        }
        OutputFormat::Table => {
            output_stacks_table(stacks, show_metadata, dir);
        }
    }
}

/// Output stacks as table with unicode box-drawing
fn output_stacks_table(stacks: &[PhotoStack], show_metadata: bool, dir: &PathBuf) {
    println!(
        "Found {} photo stack(s) in {}",
        stacks.len(),
        dir.display()
    );
    println!();

    if stacks.is_empty() {
        return;
    }

    // Calculate column widths
    let max_id = stacks.iter().map(|s| s.id.len()).max().unwrap_or(10).max(10);

    // Header
    println!(
        "┌─{}─┬─────────┬──────────┬──────┬──────┬────────┐",
        "─".repeat(max_id)
    );
    println!(
        "│ {:<max_id$} │ Format  │ Original │ Enh. │ Back │ Tags   │",
        "ID"
    );
    println!(
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

        println!(
            "│ {:<max_id$} │ {:<7} │    {:<5} │  {:<3} │  {:<3} │ {:>6} │",
            stack.id, format_str, orig, enh, back, tags
        );

        if show_metadata {
            // Show file paths
            if let Some(ref p) = stack.original {
                println!(
                    "│ {:<max_id$} │         │ {}",
                    "",
                    p.file_name().unwrap_or_default().to_string_lossy()
                );
            }
            if let Some(ref p) = stack.enhanced {
                println!(
                    "│ {:<max_id$} │         │ {}",
                    "",
                    p.file_name().unwrap_or_default().to_string_lossy()
                );
            }
            if let Some(ref p) = stack.back {
                println!(
                    "│ {:<max_id$} │         │ {}",
                    "",
                    p.file_name().unwrap_or_default().to_string_lossy()
                );
            }
        }
    }

    println!(
        "└─{}─┴─────────┴──────────┴──────┴──────┴────────┘",
        "─".repeat(max_id)
    );
}

/// Output stacks as CSV
fn output_stacks_csv(stacks: &[PhotoStack], show_metadata: bool) {
    if show_metadata {
        println!("id,format,original,enhanced,back,exif_tags,custom_tags");
    } else {
        println!("id,format,has_original,has_enhanced,has_back,tag_count");
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
            println!(
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
            println!(
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
fn output_info_table(stack: &PhotoStack) {
    let format_str = match stack.format() {
        Some(ImageFormat::Jpeg) => "JPEG",
        Some(ImageFormat::Tiff) => "TIFF",
        None => "Unknown",
    };

    println!("┌──────────────────────────────────────────────────────────────────┐");
    println!("│ Stack: {:<57} │", stack.id);
    println!("├──────────────────────────────────────────────────────────────────┤");
    println!("│ Format: {:<56} │", format_str);

    // Files section
    println!("├──────────────────────────────────────────────────────────────────┤");
    println!("│ Files:                                                           │");
    if let Some(ref p) = stack.original {
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        println!(
            "│   Original: {:<40} ({:>8}) │",
            p.file_name().unwrap_or_default().to_string_lossy(),
            format_size(size)
        );
    }
    if let Some(ref p) = stack.enhanced {
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        println!(
            "│   Enhanced: {:<40} ({:>8}) │",
            p.file_name().unwrap_or_default().to_string_lossy(),
            format_size(size)
        );
    }
    if let Some(ref p) = stack.back {
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        println!(
            "│   Back:     {:<40} ({:>8}) │",
            p.file_name().unwrap_or_default().to_string_lossy(),
            format_size(size)
        );
    }

    // EXIF tags
    if !stack.metadata.exif_tags.is_empty() {
        println!("├──────────────────────────────────────────────────────────────────┤");
        println!(
            "│ EXIF Tags ({}):",
            stack.metadata.exif_tags.len()
        );
        let width = 62;
        for (key, value) in &stack.metadata.exif_tags {
            let kv = format!("{}: {}", key, value);
            let truncated = if kv.len() > width {
                format!("{}...", &kv[..width - 3])
            } else {
                kv
            };
            println!("│   {:<width$} │", truncated);
        }
    }

    // XMP tags
    if !stack.metadata.xmp_tags.is_empty() {
        println!("├──────────────────────────────────────────────────────────────────┤");
        println!(
            "│ XMP Tags ({}):",
            stack.metadata.xmp_tags.len()
        );
        let width = 62;
        for (key, value) in &stack.metadata.xmp_tags {
            let kv = format!("{}: {}", key, value);
            let truncated = if kv.len() > width {
                format!("{}...", &kv[..width - 3])
            } else {
                kv
            };
            println!("│   {:<width$} │", truncated);
        }
    }

    // Custom tags
    if !stack.metadata.custom_tags.is_empty() {
        println!("├──────────────────────────────────────────────────────────────────┤");
        println!(
            "│ Custom Tags ({}):",
            stack.metadata.custom_tags.len()
        );
        let width = 62;
        for (key, value) in &stack.metadata.custom_tags {
            let kv = format!("{}: {}", key, value);
            let truncated = if kv.len() > width {
                format!("{}...", &kv[..width - 3])
            } else {
                kv
            };
            println!("│   {:<width$} │", truncated);
        }
    }

    println!("└──────────────────────────────────────────────────────────────────┘");
}

/// Output stack info as CSV
fn output_info_csv(stack: &PhotoStack) {
    println!("type,key,value");
    println!("id,,{}", stack.id);

    if let Some(ref p) = stack.original {
        println!("file,original,{}", p.display());
    }
    if let Some(ref p) = stack.enhanced {
        println!("file,enhanced,{}", p.display());
    }
    if let Some(ref p) = stack.back {
        println!("file,back,{}", p.display());
    }

    for (key, value) in &stack.metadata.exif_tags {
        println!("exif,{},{}", key, escape_csv(value));
    }
    for (key, value) in &stack.metadata.xmp_tags {
        println!("xmp,{},{}", key, escape_csv(value));
    }
    for (key, value) in &stack.metadata.custom_tags {
        println!("custom,{},{}", key, escape_csv(&value.to_string()));
    }
}

/// Output metadata as table
fn output_metadata_table(metadata: &Metadata) {
    println!("┌──────────────────────────────────────────────────────────────────┐");
    println!("│ Metadata                                                         │");
    println!("├──────────────────────────────────────────────────────────────────┤");

    let width = 62;

    if !metadata.exif_tags.is_empty() {
        println!(
            "│ EXIF Tags ({}):",
            metadata.exif_tags.len()
        );
        for (key, value) in &metadata.exif_tags {
            let kv = format!("{}: {}", key, value);
            let truncated = if kv.len() > width {
                format!("{}...", &kv[..width - 3])
            } else {
                kv
            };
            println!("│   {:<width$} │", truncated);
        }
    } else {
        println!("│ EXIF Tags: (none)                                                │");
    }

    println!("├──────────────────────────────────────────────────────────────────┤");

    if !metadata.xmp_tags.is_empty() {
        println!(
            "│ XMP Tags ({}):",
            metadata.xmp_tags.len()
        );
        for (key, value) in &metadata.xmp_tags {
            let kv = format!("{}: {}", key, value);
            let truncated = if kv.len() > width {
                format!("{}...", &kv[..width - 3])
            } else {
                kv
            };
            println!("│   {:<width$} │", truncated);
        }
    } else {
        println!("│ XMP Tags: (none)                                                 │");
    }

    println!("├──────────────────────────────────────────────────────────────────┤");

    if !metadata.custom_tags.is_empty() {
        println!(
            "│ Custom Tags ({}):",
            metadata.custom_tags.len()
        );
        for (key, value) in &metadata.custom_tags {
            let kv = format!("{}: {}", key, value);
            let truncated = if kv.len() > width {
                format!("{}...", &kv[..width - 3])
            } else {
                kv
            };
            println!("│   {:<width$} │", truncated);
        }
    } else {
        println!("│ Custom Tags: (none)                                              │");
    }

    println!("└──────────────────────────────────────────────────────────────────┘");
}

/// Output metadata as CSV
fn output_metadata_csv(metadata: &Metadata) {
    println!("type,key,value");

    for (key, value) in &metadata.exif_tags {
        println!("exif,{},{}", key, escape_csv(value));
    }
    for (key, value) in &metadata.xmp_tags {
        println!("xmp,{},{}", key, escape_csv(value));
    }
    for (key, value) in &metadata.custom_tags {
        println!("custom,{},{}", key, escape_csv(&value.to_string()));
    }
}

/// Format file size in human-readable format
fn format_size(bytes: u64) -> String {
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
fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
