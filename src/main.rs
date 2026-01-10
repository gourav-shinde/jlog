mod cli;
mod analyzer;
mod journalctl;
mod monitor;
mod report;
mod server;

mod helper {
    #[path = "../helper/BufferFileReader.rs"]
    mod buffer_file_reader;
    pub use buffer_file_reader::BufferedFileReader;
}

use cli::{Args, Commands};
use clap::Parser;

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Analyze { path, unit, priority, top, pattern, report, serve, port, .. } => {
            println!("jlog - Journalctl Log Analyzer\n");
            analyzer::analyze(path, unit, priority, top, pattern, report, serve, port)?;
        }
        Commands::Monitor { path, unit, priority, pattern } => {
            monitor::monitor(path, unit, priority, pattern)?;
        }
    }

    Ok(())
}
