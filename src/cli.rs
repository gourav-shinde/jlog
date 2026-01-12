use clap::{Parser, Subcommand};

/// jlog - Advanced journalctl log analyzer
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Analyze historical logs
    Analyze {
        /// Path to log file
        #[arg(short, long)]
        path: Option<String>,

        /// Analyze logs from the last N hours
        #[arg(short = 'H', long)]
        hours: Option<u32>,

        /// Filter by systemd unit/service
        #[arg(short = 'u', long)]
        unit: Option<String>,

        /// Maximum priority level to show (0=emerg, 3=err, 4=warn, 6=info, 7=debug)
        #[arg(short = 'P', long, default_value = "6")]
        priority: u8,
        
        /// Show top N most common errors
        #[arg(short = 'n', long, default_value = "10")]
        top: usize,
        
        /// Pattern to search for (regex)
        #[arg(long)]
        pattern: Option<String>,

        /// Generate HTML report to file
        #[arg(long)]
        report: Option<String>,

        /// Start live web server to view results
        #[arg(long)]
        serve: bool,

        /// Port for live server (default: 8080)
        #[arg(long, default_value = "8080")]
        port: u16,
    },
    
    /// Monitor logs in real-time
    Monitor {
        /// Path to log file
        #[arg(short, long)]
        path: Option<String>,

        /// Filter by systemd unit/service
        #[arg(short = 'u', long)]
        unit: Option<String>,

        /// Maximum priority level to show (0=emerg, 3=err, 4=warn, 6=info, 7=debug)
        #[arg(short = 'P', long, default_value = "6")]
        priority: u8,
        
        /// Pattern to search for (regex)
        #[arg(long)]
        pattern: Option<String>,
    },
}