# jlog Roadmap

## High Value
- ~~**Search within messages** — Ctrl+F to jump between matches (different from the filter, which hides non-matching lines)~~ **DONE**
- ~~**Log tailing indicator** — visual indicator showing new lines arriving, with a "jump to bottom" button when auto-scroll is off~~ **DONE**
- ~~**Bookmarks/pinning** — mark interesting log lines to revisit them quickly~~ **DONE** (B key / right-click to bookmark, Ctrl+B timeline window, gold ★ indicator)

### Bookmark refinements
- **Notes per bookmark** — annotate each bookmark with a short label (e.g. "first OOM error", "service restart"), shown in the timeline window
- **Keyboard navigation between bookmarks** — `]` / `[` to jump to next/previous bookmark from the main log view
- **"Show bookmarks only" filter** — one-click toggle to isolate bookmarked entries in the main view
- **Export timeline** — save bookmarked entries to a text/JSON file from the timeline window
- **Timestamp gap display** — show time delta between consecutive bookmarks in the timeline (e.g. "+2m 14s")
- **Navigate when filtered out** — clicking a bookmark that is hidden by filters should show it in context or give a tooltip instead of silently doing nothing
- **Bookmark count in status bar** — small `★N` indicator next to the entry count
- **Fix double hit area** — timeline rows have two overlapping click responders (message label + row rect)
- **Timestamp range filter** — filter entries between two timestamps (useful for narrowing down incidents)

## Quality of Life
- ~~**Copy row/selection** — right-click or Ctrl+C to copy a log line or selected lines to clipboard~~ **DONE**
- ~~**Show in Context** — temporarily clear filters to see surrounding log lines around a filtered entry, with one-click filter restore~~ **DONE**
- **Column resizing** — draggable column widths instead of fixed widths
- ~~**Row detail panel** — click a row to expand full message in a bottom panel (alternative to horizontal scrolling for very long messages)~~ **DONE**
- **Persistent settings** — save/load save settings, ~~connection profiles (including passwords)~~, and UI preferences to a config file (`~/.config/jlog/config.json`)

## Performance
- **Streaming save / memory cap** — for very long SSH sessions, periodically flush entries to disk and cap in-memory buffer to avoid unbounded memory growth (current approach is fine for ~50K entries)
- **Idle repaint rate** — `ctx.request_repaint()` runs unconditionally at ~60fps even when nothing is happening; use `request_repaint_after(100ms)` when not streaming/loading to reduce idle CPU usage
- **Cache regex highlight LayoutJobs** — `render_row` allocates a `vec![0u8; msg_len]` for every visible row every frame; caching `LayoutJob` per `(entry_idx, filter_hash, find_hash)` would eliminate most of this churn
- **`service_names()` clones on every frame** — `BTreeSet::iter().cloned().collect()` runs each frame inside the filter bar; cache the `Vec<String>` and only rebuild when the services set changes

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
