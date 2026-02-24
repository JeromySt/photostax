use std::path::PathBuf;

use photostax_core::backends::local::LocalRepository;
use photostax_core::repository::Repository;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let dir = match args.get(1) {
        Some(d) => PathBuf::from(d),
        None => {
            eprintln!("Usage: photostax-cli <directory> [search-text]");
            eprintln!("\nScans a directory for Epson FastFoto photo stacks.");
            std::process::exit(1);
        }
    };

    let repo = LocalRepository::new(&dir);

    let stacks = match repo.scan() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error scanning {}: {e}", dir.display());
            std::process::exit(1);
        }
    };

    // Optional text search filter
    let search_text = args.get(2);

    let filtered: Vec<_> = if let Some(query) = search_text {
        let q = photostax_core::search::SearchQuery::new().with_text(query);
        photostax_core::search::filter_stacks(&stacks, &q)
    } else {
        stacks
    };

    println!("Found {} photo stack(s) in {}", filtered.len(), dir.display());
    println!();

    for stack in &filtered {
        println!("  {} {}", if stack.has_any_image() { "📷" } else { "⚠" }, stack.id);
        if let Some(ref p) = stack.original {
            println!("    Original:  {}", p.display());
        }
        if let Some(ref p) = stack.enhanced {
            println!("    Enhanced:  {}", p.display());
        }
        if let Some(ref p) = stack.back {
            println!("    Back:      {}", p.display());
        }
        if !stack.metadata.exif_tags.is_empty() {
            println!("    EXIF tags: {}", stack.metadata.exif_tags.len());
        }
        if !stack.metadata.custom_tags.is_empty() {
            println!("    Custom tags: {}", stack.metadata.custom_tags.len());
        }
        println!();
    }
}
