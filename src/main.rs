mod cli;
#[path = "analyzer/analyze.rs"]
mod analyze;
#[path = "parsers/journalctl.rs"]
mod journalctl;
#[path = "monitor/monitor.rs"]
mod monitor;
mod helper {
    #[path = "../helper/BufferFileReader.rs"]
    mod buffer_file_reader;
    pub use buffer_file_reader::BufferedFileReader;
}

use cli::{Args, Commands};
use clap::Parser;
use analyze::analyze;
use monitor::monitor;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Analyze { path, unit, priority, top, pattern, .. } => {
            println!("jlog - Journalctl Log Analyzer\n");
            analyze(path, unit, priority, top, pattern)?;
        }
        Commands::Monitor { path, unit, priority, pattern } => {
            monitor(path, unit, priority, pattern)?;
        }
    }

    Ok(())
}
