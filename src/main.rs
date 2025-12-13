mod cli;
#[path = "analyzer/analyze.rs"]
mod analyze;

use cli::{Args, Commands};
use clap::Parser;
use analyze::Analyze;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    
    println!("jlog - Journalctl Log Analyzer");

    match args.command {
        Commands::Analyze { path, hours, top, .. } => {
            println!("Analyzing logs...\n");
            Analyze( path );
            // Your analyze logic here
        }
        Commands::Monitor { .. } => {
            println!("Monitoring logs in real-time...\n");
            // Your monitor logic here
        }
    }

    Ok(())
}
