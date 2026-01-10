use std::io::{Read, Write};
use std::net::TcpListener;
use colored::Colorize;

use crate::analyzer::state::AnalysisState;
use crate::report::build_html;

pub fn start_server(state: &AnalysisState, port: u16, lines_read: usize) -> anyhow::Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr)?;

    println!("\n{}", "─".repeat(60).dimmed());
    println!("{}", "🌐 Live Server Started".green().bold());
    println!("   Open in browser: {}", format!("http://{}", addr).cyan());
    println!("   Press {} to stop", "Ctrl+C".yellow());
    println!("{}", "─".repeat(60).dimmed());

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut buffer = [0; 1024];
                if stream.read(&mut buffer).is_ok() {
                    let html = build_html(state, lines_read);
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                        html.len(),
                        html
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            }
            Err(e) => {
                eprintln!("{}", format!("Connection error: {}", e).red());
            }
        }
    }

    Ok(())
}
