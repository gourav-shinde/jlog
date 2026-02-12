# jlog Roadmap

## High Value
- ~~**Search within messages** — Ctrl+F to jump between matches (different from the filter, which hides non-matching lines)~~ **DONE**
- ~~**Log tailing indicator** — visual indicator showing new lines arriving, with a "jump to bottom" button when auto-scroll is off~~ **DONE**
- **Bookmarks/pinning** — mark interesting log lines to revisit them quickly
- **Timestamp range filter** — filter entries between two timestamps (useful for narrowing down incidents)

## Quality of Life
- **Copy row/selection** — right-click or Ctrl+C to copy a log line or selected lines to clipboard
- **Column resizing** — draggable column widths instead of fixed widths
- ~~**Row detail panel** — click a row to expand full message in a bottom panel (alternative to horizontal scrolling for very long messages)~~ **DONE**
- **Persistent settings** — save/load save settings, ~~connection profiles (including passwords)~~, and UI preferences to a config file (`~/.config/jlog/config.json`)

## Power User
- **Multiple SSH connections** — tabs for different hosts, view side-by-side
- **Log correlation** — highlight entries within N seconds of a selected entry across services
- **Export filtered view** — export just what's currently visible (quick "copy visible to clipboard")
- **Stats panel** — entry count per service, error rate over time, simple sparkline charts

## Polish
- ~~**Keyboard shortcuts** — `j/k` for row navigation, `/` to focus filter, `g/G` for top/bottom~~ **DONE** (Help menu with shortcuts & about dialog)
- **Reopen saved logs** — saved JSON and plaintext log files can be reopened via File > Open **DONE**
- **Auto-save on exit** — logs are saved automatically when closing the window during an active session **DONE**
- **Color themes** — light mode, custom color schemes
- ~~**Connection history** — remember recent SSH connections for quick reconnect~~ **DONE** (via connection profiles, SSH menu, and status bar reconnect buttons)
