# jlog

Advanced journalctl log analyzer with pattern detection and real-time monitoring.

## Features

- **Streaming Analysis** - Process large log files (1GB+) with minimal memory usage
- **Dynamic Pattern Detection** - Automatically detect spikes, bursts, recurring issues, and anomalies
- **Multiple Log Formats** - Supports plain text syslog, journalctl short-precise, and JSON formats
- **Real-time Monitoring** - Tail logs and see events as they happen
- **Smart Filtering** - Filter by service, priority level, or regex patterns
- **Color-coded Output** - Visual priority indicators and bar charts
- **HTML Reports** - Generate interactive reports with Chart.js visualizations
- **Configurable Time Buckets** - Adjust time-series granularity (1min to 1hr) in web UI
- **Live Web Server** - View analysis results in your browser with auto-refresh

## Installation

```bash
cargo build --release
```

The binary will be at `target/release/jlog`.

## Usage

### Analyze Historical Logs

Analyze any log file (plain text syslog or JSON format):

```bash
# Analyze plain text syslog/journalctl output
jlog analyze --path /var/log/syslog

# Analyze journalctl short-precise format
journalctl -o short-precise > /tmp/logs.txt
jlog analyze --path /tmp/logs.txt

# Analyze JSON export
journalctl -o json > /tmp/logs.json
jlog analyze --path /tmp/logs.json
```

#### Options

| Flag | Short | Description | Default |
|------|-------|-------------|---------|
| `--path` | `-p` | Path to log file (text or JSON) | Required |
| `--priority` | `-P` | Max priority level (0=emerg to 7=debug) | `6` (info) |
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

⚠ DYNAMIC PATTERNS DETECTED
  🔴 [Spike] Spike of 89 at 2024-01-15 14:30, avg 2.1/bucket
      upstream timed out (110: Connection timed out)
  🟡 [Recurring] Recurring 523 times across 85% of time range
      Failed password for invalid user admin from <IP>...
  🟡 [Burst] 45 occurrences in only 3 time windows
      Connection refused
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

## Dynamic Pattern Detection

jlog uses intelligent analysis to automatically detect anomalies in your logs - no hardcoded patterns required. It analyzes message frequency and distribution over time to identify:

| Pattern | Icon | Description |
|---------|------|-------------|
| **Spike** | 📈 | Sudden increase in message frequency (3x above average) |
| **Burst** | 💥 | Many occurrences concentrated in a short time window |
| **Recurring** | 🔄 | Message appears consistently across the time range |
| **Increasing** | 📊 | Message rate growing over time (2x increase in second half) |
| **High Volume** | 🔥 | Message dominates the error log (>25% of all errors) |

### How It Works

1. **Collects data** at minute-level granularity during log processing
2. **Analyzes distribution** of each error/warning message across time buckets
3. **Detects anomalies** by comparing against statistical baselines:
   - Spikes: max bucket value > 3x average
   - Bursts: occurrences in <30% of time range
   - Recurring: present in >40% of time buckets
   - Increasing: second half rate > 2x first half

### Benefits

- **Works with any log format** - No need to define patterns for your specific system
- **Adapts automatically** - Detects issues unique to your application
- **Prioritizes by severity** - Critical issues shown first
- **Shows context** - Displays the actual message and statistics

## Performance

- **Streaming architecture** - Processes entries one at a time
- **Memory efficient** - Uses ~5-10MB regardless of file size
- **128KB I/O buffer** - Optimized for large sequential reads
- **Progress indicator** - Shows progress for files >10MB

## Supported Log Formats

jlog automatically detects and parses multiple log formats:

### Plain Text Syslog (Default)
Standard syslog format used by most Linux systems:
```
Jan 10 16:42:10 hostname service[pid]: message
```

```bash
# Direct file analysis
jlog analyze --path /var/log/syslog
jlog analyze --path /var/log/messages
```

### Journalctl Short-Precise
Includes microseconds (journalctl -o short-precise):
```
Jan 06 15:51:19.246531 hostname service[pid]: message
```

```bash
journalctl -o short-precise > logs.txt
jlog analyze --path logs.txt
```

### JSON Format
Journalctl JSON export (one object per line):
```bash
journalctl -o json > logs.json
journalctl -o json --since "1 hour ago" > recent.json
jlog analyze --path logs.json
```

## Roadmap

### Completed

- ✅ Time-series bucketing (configurable 1min to 1hr granularity)
- ✅ Generate HTML reports with embedded charts
- ✅ Live web server for interactive viewing
- ✅ Real-time monitoring mode
- ✅ Dynamic pattern detection (spikes, bursts, recurring, increasing)
- ✅ Plain text syslog format support
- ✅ Journalctl short-precise format support (with microseconds)
- ✅ Configurable bucket size selector in web UI

### Planned Features

**Advanced Analysis**
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
- Apache/Nginx access logs
- Custom log format definitions

**Configuration**
- User-defined alert rules (YAML/TOML)
- Custom pattern definitions
- Saved filter presets

### Ideas

- **"Explain this error"** - Integrate with LLM API to explain common systemd errors
- **Cluster similar errors** - Group related error messages automatically
- **Plugin system** - Let users write custom analyzers

## License

MIT
