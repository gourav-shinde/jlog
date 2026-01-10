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
    </style>
</head>
<body>
    <div class="container">
        <h1>📊 jlog Analysis Report</h1>

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

    <script>
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
        time_labels.join(","),
        time_totals.join(","),
        time_errors.join(","),
        time_warnings.join(","),
        priority_labels.join(","),
        priority_counts.join(","),
        service_labels.join(","),
        service_counts.join(",")
    )
}
