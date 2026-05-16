# jlog

Native desktop log viewer for journalctl/syslog logs. Open local log files or SSH to a remote server and stream `journalctl` output live.

## Install

### Pre-built binary (no Rust required)

```bash
git clone https://github.com/gourav-shinde/jlog.git
cd jlog
sudo ./install.sh
```

`install.sh` will download the latest pre-built binary from GitHub Releases if available, otherwise build from source. It installs to `/opt/jlog` and adds it to your PATH.

To uninstall:

```bash
sudo ./uninstall.sh
```

### Build from source

Requires Rust and OpenSSL dev libraries (`libssl-dev` on Debian/Ubuntu).

```bash
cargo build --release
./target/release/jlog
```

## Usage

```bash
# Launch empty, then use File > Open File or SSH > Connect SSH
jlog

# Open a log file directly
jlog -f /path/to/logfile.log
jlog --file /path/to/logfile.log

# Connect to a saved SSH profile directly
jlog -p myserver
jlog --profile myserver

# Show help
jlog -h
jlog --help
```

SSH profiles are managed via SSH > Connect SSH and saved to `~/.config/jlog/profiles.json`.

On WSL2, the app uses X11 by default. Override with `WINIT_UNIX_BACKEND=wayland` if needed.

## Features

- Open saved log files (syslog, journalctl JSON, plain text)
- SSH to remote servers and stream journalctl output live
- Regex filtering with AND/OR/NOT combine modes
- Filter by service name and priority level
- Virtual-scrolling log table (handles 100k+ entries)
- Regex match highlighting in messages
- Quick-pattern buttons for common searches (errors, warnings, SSH, kernel, systemd)
- Bookmark entries (B key or right-click) and navigate via the bookmark timeline (Ctrl+B)
- "Always show bookmarks" setting to pin bookmarked entries through active filters
- Auto-save logs on SSH disconnect

## Supported Formats

- Plain text syslog (`Mon DD HH:MM:SS hostname service[pid]: message`)
- journalctl short-precise (with microseconds)
- journalctl JSON (`journalctl -o json`)

## License

MIT
