//! The kessel command line: agents only.

use clap::Parser;

/// Kessel answers agents about SWTOR's game data.
#[derive(Parser)]
#[command(version)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
