use std::fs::File;
use std::io::Write;
use crate::analyzer::state::{AnalysisState, Severity};

pub fn generate_html_report(state: &AnalysisState, output_path: &str, lines_read: usize) -> anyhow::Result<()> {
    let html = build_html(state, lines_read);
    let mut file = File::create(output_path)?;
    file.write_all(html.as_bytes())?;
    Ok(())
}

pub fn build_html(state: &AnalysisState, lines_read: usize) -> String {
    let critical = state.entries_by_priority[0] + state.entries_by_priority[1] + state.entries_by_priority[2];
    let errors = state.entries_by_priority[3];
    let warnings = state.entries_by_priority[4];

    let time_series = state.sorted_time_series();
    let time_labels: Vec<_> = time_series.iter().map(|(t, _)| format!("\"{}\"", t)).collect();
    let time_totals: Vec<_> = time_series.iter().map(|(_, b)| b.total.to_string()).collect();
    let time_errors: Vec<_> = time_series.iter().map(|(_, b)| b.errors.to_string()).collect();
    let time_warnings: Vec<_> = time_series.iter().map(|(_, b)| b.warnings.to_string()).collect();

    let top_services = state.top_services(10);
    let service_labels: Vec<_> = top_services.iter().map(|(s, _)| format!("\"{}\"", s)).collect();
    let service_counts: Vec<_> = top_services.iter().map(|(_, c)| c.to_string()).collect();

    let priority_labels = vec!["\"EMERG\"", "\"ALERT\"", "\"CRIT\"", "\"ERR\"", "\"WARN\"", "\"NOTICE\"", "\"INFO\"", "\"DEBUG\""];
    let priority_counts: Vec<_> = state.entries_by_priority.iter().map(|c| c.to_string()).collect();

    let top_errors = state.top_errors(20);
    let error_rows: String = top_errors.iter().enumerate().map(|(i, (msg, count))| {
        let escaped_msg = msg.replace('<', "&lt;").replace('>', "&gt;");
        format!("<tr><td>{}</td><td class=\"count\">{}</td><td>{}</td></tr>", i + 1, count, escaped_msg)
    }).collect::<Vec<_>>().join("\n");

    let patterns = state.get_patterns();
    let pattern_cards: String = patterns.iter().map(|p| {
        let (class, icon) = match p.severity {
            Severity::Critical => ("critical", "🔴"),
            Severity::Warning => ("warning", "🟡"),
            Severity::Info => ("info", "🔵"),
        };
        format!(r#"<div class="pattern-card {}"><span class="icon">{}</span><strong>{}</strong><p>{}</p></div>"#,
            class, icon, p.name, p.description)
    }).collect::<Vec<_>>().join("\n");

    // Build message trends data for chart
    let all_buckets = state.all_time_buckets();
    let message_trends = state.top_message_trends(10);

    let trend_labels: Vec<_> = all_buckets.iter().map(|t| format!("\"{}\"", t)).collect();

    // Generate datasets for each message
    let colors = ["#58a6ff", "#f85149", "#d29922", "#3fb950", "#a371f7", "#f778ba", "#79c0ff", "#ffa657", "#56d364", "#ff7b72"];

    let trend_datasets: String = message_trends.iter().enumerate().map(|(i, (msg, buckets))| {
        let color = colors[i % colors.len()];
        // Create data array matching all_buckets order
        let data: Vec<String> = all_buckets.iter().map(|bucket| {
            buckets.iter()
                .find(|(b, _)| b == bucket)
                .map(|(_, count)| count.to_string())
                .unwrap_or_else(|| "0".to_string())
        }).collect();

        let escaped_label = msg.replace('"', "\\\"").chars().take(50).collect::<String>();
        format!(
            r#"{{ label: "{}{}", data: [{}], borderColor: '{}', backgroundColor: '{}22', fill: false, tension: 0.1 }}"#,
            escaped_label,
            if msg.len() > 50 { "..." } else { "" },
            data.join(","),
            color,
            color
        )
    }).collect::<Vec<_>>().join(",\n                    ");

    // Build trends table
    let trends_table_rows: String = message_trends.iter().enumerate().map(|(i, (msg, buckets))| {
        let total: usize = buckets.iter().map(|(_, c)| *c).sum();
        let escaped_msg = msg.replace('<', "&lt;").replace('>', "&gt;");
        let sparkline: String = buckets.iter().map(|(_, c)| {
            let height = if total > 0 { (*c as f64 / total as f64 * 100.0).min(100.0) } else { 0.0 };
            format!(r#"<div class="spark-bar" style="height: {}%"></div>"#, height.max(5.0))
        }).collect::<Vec<_>>().join("");

        format!(
            r#"<tr><td>{}</td><td class="count">{}</td><td class="sparkline">{}</td><td>{}</td></tr>"#,
            i + 1, total, sparkline, escaped_msg
        )
    }).collect::<Vec<_>>().join("\n");

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>jlog Report</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0d1117; color: #c9d1d9; padding: 20px; }}
        .container {{ max-width: 1400px; margin: 0 auto; }}
        h1 {{ color: #58a6ff; margin-bottom: 20px; }}
        h2 {{ color: #8b949e; margin: 20px 0 10px; font-size: 1.1em; text-transform: uppercase; }}

        /* Tabs */
        .tabs {{ display: flex; gap: 5px; margin-bottom: 20px; border-bottom: 1px solid #30363d; }}
        .tab {{ padding: 12px 24px; background: transparent; border: none; color: #8b949e; font-size: 1em; cursor: pointer; border-bottom: 2px solid transparent; transition: all 0.2s; }}
        .tab:hover {{ color: #c9d1d9; background: #21262d; }}
        .tab.active {{ color: #58a6ff; border-bottom-color: #58a6ff; }}
        .tab-content {{ display: none; }}
        .tab-content.active {{ display: block; }}

        .summary {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 15px; margin-bottom: 30px; }}
        .card {{ background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 20px; text-align: center; }}
        .card .value {{ font-size: 2em; font-weight: bold; }}
        .card .label {{ color: #8b949e; font-size: 0.9em; }}
        .card.critical .value {{ color: #f85149; }}
        .card.error .value {{ color: #f85149; }}
        .card.warning .value {{ color: #d29922; }}
        .card.info .value {{ color: #58a6ff; }}
        .charts {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(400px, 1fr)); gap: 20px; margin-bottom: 30px; }}
        .chart-container {{ background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 20px; }}
        .chart-container.full-width {{ grid-column: 1 / -1; }}
        .patterns {{ display: flex; flex-wrap: wrap; gap: 10px; margin-bottom: 30px; }}
        .pattern-card {{ background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 15px; flex: 1; min-width: 200px; }}
        .pattern-card.critical {{ border-color: #f85149; }}
        .pattern-card.warning {{ border-color: #d29922; }}
        .pattern-card.info {{ border-color: #58a6ff; }}
        .pattern-card .icon {{ font-size: 1.5em; margin-right: 10px; }}
        .pattern-card strong {{ color: #c9d1d9; }}
        .pattern-card p {{ color: #8b949e; margin-top: 5px; font-size: 0.9em; }}
        table {{ width: 100%; border-collapse: collapse; background: #161b22; border: 1px solid #30363d; border-radius: 8px; overflow: hidden; }}
        th, td {{ padding: 12px; text-align: left; border-bottom: 1px solid #30363d; }}
        th {{ background: #21262d; color: #8b949e; font-weight: 600; }}
        td.count {{ color: #f85149; font-weight: bold; width: 80px; }}
        tr:hover {{ background: #21262d; }}

        /* Sparkline mini chart */
        .sparkline {{ display: flex; align-items: flex-end; gap: 2px; height: 30px; width: 120px; }}
        .spark-bar {{ width: 8px; background: #58a6ff; border-radius: 2px 2px 0 0; min-height: 2px; }}

        /* Legend for trends */
        .trends-legend {{ display: flex; flex-wrap: wrap; gap: 10px; margin-bottom: 15px; padding: 15px; background: #161b22; border: 1px solid #30363d; border-radius: 8px; }}
        .legend-item {{ display: flex; align-items: center; gap: 5px; font-size: 0.85em; }}
        .legend-color {{ width: 12px; height: 12px; border-radius: 2px; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>📊 jlog Analysis Report</h1>

        <div class="tabs">
            <button class="tab active" onclick="showTab('overview')">Overview</button>
            <button class="tab" onclick="showTab('trends')">Message Trends</button>
        </div>

        <!-- Overview Tab -->
        <div id="overview" class="tab-content active">
            <h2>Summary</h2>
            <div class="summary">
                <div class="card info"><div class="value">{}</div><div class="label">Lines Read</div></div>
                <div class="card info"><div class="value">{}</div><div class="label">Entries Matched</div></div>
                <div class="card critical"><div class="value">{}</div><div class="label">Critical</div></div>
                <div class="card error"><div class="value">{}</div><div class="label">Errors</div></div>
                <div class="card warning"><div class="value">{}</div><div class="label">Warnings</div></div>
            </div>

            <h2>Patterns Detected</h2>
            <div class="patterns">
                {}
            </div>

            <h2>Charts</h2>
            <div class="charts">
                <div class="chart-container">
                    <canvas id="timeChart"></canvas>
                </div>
                <div class="chart-container">
                    <canvas id="priorityChart"></canvas>
                </div>
                <div class="chart-container">
                    <canvas id="serviceChart"></canvas>
                </div>
            </div>

            <h2>Top Error Messages</h2>
            <table>
                <thead><tr><th>#</th><th>Count</th><th>Message</th></tr></thead>
                <tbody>{}</tbody>
            </table>
        </div>

        <!-- Trends Tab -->
        <div id="trends" class="tab-content">
            <h2>Message Frequency Over Time</h2>
            <p style="color: #8b949e; margin-bottom: 20px;">Track how frequently each error message appears over time to identify patterns and spikes.</p>

            <div class="charts">
                <div class="chart-container full-width">
                    <canvas id="trendsChart"></canvas>
                </div>
            </div>

            <h2>Top Messages with Trend</h2>
            <table>
                <thead><tr><th>#</th><th>Total</th><th>Trend</th><th>Message</th></tr></thead>
                <tbody>{}</tbody>
            </table>
        </div>
    </div>

    <script>
        // Tab switching
        function showTab(tabId) {{
            document.querySelectorAll('.tab-content').forEach(el => el.classList.remove('active'));
            document.querySelectorAll('.tab').forEach(el => el.classList.remove('active'));
            document.getElementById(tabId).classList.add('active');
            document.querySelector(`[onclick="showTab('${{tabId}}')"]`).classList.add('active');
        }}

        // Time series chart
        new Chart(document.getElementById('timeChart'), {{
            type: 'line',
            data: {{
                labels: [{}],
                datasets: [
                    {{ label: 'Total', data: [{}], borderColor: '#58a6ff', fill: false }},
                    {{ label: 'Errors', data: [{}], borderColor: '#f85149', fill: false }},
                    {{ label: 'Warnings', data: [{}], borderColor: '#d29922', fill: false }}
                ]
            }},
            options: {{
                responsive: true,
                plugins: {{ title: {{ display: true, text: 'Log Volume Over Time', color: '#c9d1d9' }} }},
                scales: {{
                    x: {{ ticks: {{ color: '#8b949e' }}, grid: {{ color: '#30363d' }} }},
                    y: {{ ticks: {{ color: '#8b949e' }}, grid: {{ color: '#30363d' }} }}
                }}
            }}
        }});

        // Priority distribution
        new Chart(document.getElementById('priorityChart'), {{
            type: 'doughnut',
            data: {{
                labels: [{}],
                datasets: [{{ data: [{}], backgroundColor: ['#f85149','#f85149','#f85149','#da3633','#d29922','#58a6ff','#8b949e','#484f58'] }}]
            }},
            options: {{
                responsive: true,
                plugins: {{ title: {{ display: true, text: 'Priority Distribution', color: '#c9d1d9' }}, legend: {{ labels: {{ color: '#c9d1d9' }} }} }}
            }}
        }});

        // Top services
        new Chart(document.getElementById('serviceChart'), {{
            type: 'bar',
            data: {{
                labels: [{}],
                datasets: [{{ label: 'Log Count', data: [{}], backgroundColor: '#58a6ff' }}]
            }},
            options: {{
                indexAxis: 'y',
                responsive: true,
                plugins: {{ title: {{ display: true, text: 'Top Services', color: '#c9d1d9' }}, legend: {{ display: false }} }},
                scales: {{
                    x: {{ ticks: {{ color: '#8b949e' }}, grid: {{ color: '#30363d' }} }},
                    y: {{ ticks: {{ color: '#8b949e' }}, grid: {{ color: '#30363d' }} }}
                }}
            }}
        }});

        // Message trends chart
        new Chart(document.getElementById('trendsChart'), {{
            type: 'line',
            data: {{
                labels: [{}],
                datasets: [
                    {}
                ]
            }},
            options: {{
                responsive: true,
                interaction: {{ mode: 'index', intersect: false }},
                plugins: {{
                    title: {{ display: true, text: 'Message Frequency Over Time', color: '#c9d1d9' }},
                    legend: {{
                        position: 'bottom',
                        labels: {{ color: '#c9d1d9', boxWidth: 12, padding: 15 }}
                    }}
                }},
                scales: {{
                    x: {{ ticks: {{ color: '#8b949e' }}, grid: {{ color: '#30363d' }} }},
                    y: {{ ticks: {{ color: '#8b949e' }}, grid: {{ color: '#30363d' }}, beginAtZero: true }}
                }}
            }}
        }});
    </script>
</body>
</html>"#,
        lines_read,
        state.total_entries,
        critical,
        errors,
        warnings,
        if patterns.is_empty() { "<div class=\"pattern-card info\">No concerning patterns detected.</div>".to_string() } else { pattern_cards },
        error_rows,
        trends_table_rows,
        time_labels.join(","),
        time_totals.join(","),
        time_errors.join(","),
        time_warnings.join(","),
        priority_labels.join(","),
        priority_counts.join(","),
        service_labels.join(","),
        service_counts.join(","),
        trend_labels.join(","),
        trend_datasets
    )
}
