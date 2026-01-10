mod cli;
#[path = "analyzer/analyze.rs"]
mod analyze;
#[path = "parsers/journalctl.rs"]
mod journalctl;

use cli::{Args, Commands};
use clap::Parser;
use analyze::analyze;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    println!("jlog - Journalctl Log Analyzer\n");

    match args.command {
        Commands::Analyze { path, unit, priority, top, pattern, .. } => {
            analyze(path, unit, priority, top, pattern)?;
        }
        Commands::Monitor { .. } => {
            println!("Monitoring logs in real-time...\n");
            println!("(Monitor mode not yet implemented)");
        }
    }

    Ok(())
}
