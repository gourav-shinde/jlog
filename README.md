# jlog

Advanced journalctl log analyzer with pattern detection and real-time monitoring.

## Features

- **Streaming Analysis** - Process large log files (1GB+) with minimal memory usage
- **Pattern Detection** - Automatically detect SSH brute force, OOM events, disk issues, timeouts
- **Real-time Monitoring** - Tail logs and see events as they happen
- **Smart Filtering** - Filter by service, priority level, or regex patterns
- **Color-coded Output** - Visual priority indicators and bar charts
- **HTML Reports** - Generate interactive reports with Chart.js visualizations
- **Live Web Server** - View analysis results in your browser with auto-refresh

## Installation

```bash
cargo build --release
```

The binary will be at `target/release/jlog`.

## Usage

### Analyze Historical Logs

Analyze a journalctl JSON export:

```bash
# Export logs from journalctl
journalctl -o json > /tmp/logs.json

# Analyze with jlog
jlog analyze --path /tmp/logs.json
```

#### Options

| Flag | Short | Description | Default |
|------|-------|-------------|---------|
| `--path` | `-p` | Path to JSON log file | Required |
| `--priority` | `-P` | Max priority level (0=emerg to 7=debug) | `3` (errors) |
| `--unit` | `-u` | Filter by systemd unit/service name | None |
| `--top` | `-n` | Show top N errors/services | `10` |
| `--pattern` | | Regex pattern to filter messages | None |
| `--report` | | Generate HTML report to specified file | None |
| `--serve` | | Start live web server to view results | `false` |
| `--port` | | Port for live server | `8080` |

#### Examples

```bash
# Show all log levels (including info/debug)
jlog analyze --path logs.json --priority 7

# Filter by service
jlog analyze --path logs.json --unit nginx --priority 7

# Filter by regex pattern
jlog analyze --path logs.json --pattern "Failed password"

# Show top 20 errors
jlog analyze --path logs.json --top 20

# Combine filters
jlog analyze --path logs.json --unit sshd --priority 4 --pattern "invalid user"

# Generate HTML report
jlog analyze --path logs.json --priority 7 --report report.html

# Start live web server
jlog analyze --path logs.json --serve

# Start server on custom port
jlog analyze --path logs.json --serve --port 3000

# Generate report AND start server
jlog analyze --path logs.json --report report.html --serve
```

#### Sample Output

```
jlog - Journalctl Log Analyzer

Analyzing: ./logs.json
────────────────────────────────────────────────────────────

SUMMARY
  Lines read:             150000
  Entries matched:        47532
  Critical/Alert/Emerg:   12
  Errors:                 856
  Warnings:               4521

PRIORITY DISTRIBUTION
  CRIT    ██ 12
  ERR     ████████ 856
  WARNING ███████████████████████ 4521
  INFO    ██████████████████████████████ 42143

TOP SERVICES
  nginx           ██████████████████████████████ 15234
  sshd            █████████████████████████ 12453
  systemd         ████████████ 8234

TOP ERROR MESSAGES
  1. [523x] Failed password for invalid user admin from <IP> port <PORT> ssh2
  2. [89x] upstream timed out (110: Connection timed out)
  3. [45x] Connection refused

⚠ PATTERNS DETECTED
  🔴 SSH Brute Force Attempt: 523 failed password attempts
  🔴 Out of Memory: 3 OOM killer events
  🟡 Connection Timeouts: 89 timeout events
```

### HTML Reports & Live Server

Generate interactive HTML reports with charts and visualizations.

#### Static HTML Report

```bash
jlog analyze --path logs.json --report report.html
```

Creates a self-contained HTML file with:
- Summary cards (total, errors, warnings, critical)
- Line chart showing log volume over time
- Doughnut chart for priority distribution
- Bar chart for top services
- Pattern detection alerts
- Searchable error table

#### Live Web Server

```bash
jlog analyze --path logs.json --serve
# Opens at http://127.0.0.1:8080
```

Starts an HTTP server serving the same interactive report. Useful for:
- Viewing results in a browser with better formatting
- Sharing with team members on the same network
- Quick visualization without saving files

### Real-time Monitoring

Monitor logs as they happen:

```bash
# Monitor from stdin (pipe from journalctl)
journalctl -f -o json | jlog monitor

# Monitor a file (tail -f style)
jlog monitor --path /var/log/journal.json

# Filter real-time logs
journalctl -f -o json | jlog monitor --unit nginx --priority 4
```

#### Options

| Flag | Short | Description | Default |
|------|-------|-------------|---------|
| `--path` | `-p` | Path to log file (tails for new entries) | stdin |
| `--priority` | `-P` | Max priority level (0=emerg to 7=debug) | `3` |
| `--unit` | `-u` | Filter by systemd unit/service name | None |
| `--pattern` | | Regex pattern to filter messages | None |

#### Examples

```bash
# Monitor all errors from any service
journalctl -f -o json | jlog monitor

# Monitor specific service with warnings
journalctl -f -o json -u nginx | jlog monitor --priority 4

# Watch for specific patterns
journalctl -f -o json | jlog monitor --pattern "connection refused"

# Tail a log file
jlog monitor --path /tmp/logs.json
```

#### Sample Output

```
────────────────────────────────────────────────────────────────────────────────
jlog - Real-time Log Monitor
────────────────────────────────────────────────────────────────────────────────
Reading from: stdin
Pipe journalctl output: journalctl -f -o json | jlog monitor
────────────────────────────────────────────────────────────────────────────────

ERR     nginx           upstream timed out (110: Connection timed out)
  ⚠ Error from nginx
WARN    sshd            Failed password for invalid user admin from 192.168.1.100
  ⚠ SSH auth failure detected
ERR     postgresql      connection limit exceeded for non-superusers
  ⚠ Error from postgresql
```

## Priority Levels

| Level | Name | Description |
|-------|------|-------------|
| 0 | EMERG | System is unusable |
| 1 | ALERT | Action must be taken immediately |
| 2 | CRIT | Critical conditions |
| 3 | ERR | Error conditions (default filter) |
| 4 | WARNING | Warning conditions |
| 5 | NOTICE | Normal but significant |
| 6 | INFO | Informational |
| 7 | DEBUG | Debug-level messages |

## Pattern Detection

jlog automatically detects these patterns:

| Pattern | Severity | Trigger |
|---------|----------|---------|
| SSH Brute Force | 🔴 Critical (10+) / 🟡 Warning (3+) | "Failed password" messages |
| Out of Memory | 🔴 Critical | OOM killer events |
| Service Restarts | 🟡 Warning | Restart events (2+) |
| Connection Timeouts | 🟡 Warning | Timeout messages (2+) |
| Disk Issues | 🟡 Warning | Disk errors or >90% usage |
| Firewall Blocks | 🔵 Info | UFW BLOCK messages (2+) |

## Performance

- **Streaming architecture** - Processes entries one at a time
- **Memory efficient** - Uses ~5-10MB regardless of file size
- **128KB I/O buffer** - Optimized for large sequential reads
- **Progress indicator** - Shows progress for files >10MB

## Input Format

jlog expects journalctl JSON output (one JSON object per line):

```bash
# Generate compatible input
journalctl -o json > logs.json
journalctl -o json --since "1 hour ago" > recent.json
journalctl -o json -u nginx -u postgresql > services.json
```

## Roadmap

### Completed

- ✅ Time-series bucketing (logs per hour)
- ✅ Generate HTML reports with embedded charts
- ✅ Live web server for interactive viewing
- ✅ Real-time monitoring mode
- ✅ Pattern detection (SSH brute force, OOM, timeouts, etc.)

### Planned Features

**Advanced Analysis**
- Anomaly detection (baseline vs. current behavior)
- Service health scoring algorithm
- Correlation detection ("Service X fails 30 seconds after Service Y restarts")

**Export & Reporting**
- Export to JSON/CSV for further analysis
- Create summaries suitable for incident reports

**Interactive TUI Mode**
- Full-screen terminal UI (like htop)
- Live filtering and searching
- Visual statistics overlays
- Scrollable log viewer

**Extended Format Support**
- Syslog parsing
- Apache/Nginx access logs
- Custom log format definitions

**Configuration**
- User-defined alert rules (YAML/TOML)
- Custom pattern definitions
- Saved filter presets

### Ideas

- **"Explain this error"** - Integrate with LLM API to explain common systemd errors
- **Machine learning lite** - Learn "normal" baseline patterns, flag deviations
- **Cluster similar errors** - Group related error messages automatically
- **Plugin system** - Let users write custom analyzers

## License

MIT
