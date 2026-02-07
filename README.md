# jlog

Native desktop log viewer for journalctl/syslog logs. Open local log files or SSH to a remote server and stream `journalctl` output live.

## Build

Requires OpenSSL dev libraries (`libssl-dev` on Debian/Ubuntu).

```bash
cargo build --release
```

## Usage

```bash
# Launch with a file
./target/release/jlog path/to/logfile.log

# Launch empty, then use File > Open File or File > Connect SSH
./target/release/jlog
```

On WSL2, the app uses X11 by default. Override with `WINIT_UNIX_BACKEND=wayland` if needed.

## Features

- Open saved log files (syslog, journalctl JSON, plain text)
- SSH to remote servers and stream journalctl output live
- Regex filtering with AND/OR/NOT combine modes
- Filter by service name and priority level
- Virtual-scrolling log table (handles 100k+ entries)
- Regex match highlighting in messages
- Quick-pattern buttons for common searches (errors, warnings, SSH, kernel, systemd)

## Supported Formats

- Plain text syslog (`Mon DD HH:MM:SS hostname service[pid]: message`)
- journalctl short-precise (with microseconds)
- journalctl JSON (`journalctl -o json`)

## License

MIT
