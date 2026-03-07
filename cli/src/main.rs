//! CLI binary entry point. See [`photostax_cli`] for the library implementation.

use clap::Parser;

fn main() {
    let cli = photostax_cli::Cli::parse();
    let exit_code = photostax_cli::run_cli(&cli, &mut std::io::stdout(), &mut std::io::stderr());
    std::process::exit(exit_code);
}
