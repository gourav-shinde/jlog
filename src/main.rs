mod analyzer;
mod app;
mod background;
mod journalctl;
mod ui;
mod workers;

use std::io;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut pending_file = None;
    let mut pending_ssh_profile = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("Usage: jlog [OPTIONS]");
                println!();
                println!("Options:");
                println!("  -f, --file <PATH>       Open a log file directly");
                println!("  -p, --profile <NAME>    Connect using a saved SSH profile");
                println!("  -h, --help              Print this help message");
                return Ok(());
            }
            "-f" | "--file" => pending_file = args.next(),
            "-p" | "--profile" => pending_ssh_profile = args.next(),
            _ => {}
        }
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = app::App::new(pending_file, pending_ssh_profile).run(&mut terminal);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}
