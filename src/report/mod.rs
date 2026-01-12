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
        let class = match p.severity {
            Severity::Critical => "critical",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        let escaped_msg = p.message.replace('<', "&lt;").replace('>', "&gt;");
        format!(
            r#"<div class="pattern-card {}">
                <div class="pattern-header"><span class="icon">{}</span><span class="pattern-type">{}</span></div>
                <p class="pattern-desc">{}</p>
                <p class="pattern-msg">{}</p>
            </div>"#,
            class, p.pattern_type.icon(), p.pattern_type.label(), p.description, escaped_msg
        )
    }).collect::<Vec<_>>().join("\n");

    // Build raw minute-level data as JSON for client-side aggregation
    let time_series = state.sorted_time_series();
    let raw_time_data: String = time_series.iter().map(|(t, b)| {
        format!(r#"{{"t":"{}","total":{},"errors":{},"warnings":{}}}"#, t, b.total, b.errors, b.warnings)
    }).collect::<Vec<_>>().join(",");

    // Build message trends raw data
    let message_trends = state.top_message_trends(10);
    let colors = ["#58a6ff", "#f85149", "#d29922", "#3fb950", "#a371f7", "#f778ba", "#79c0ff", "#ffa657", "#56d364", "#ff7b72"];

    let raw_trends_data: String = message_trends.iter().enumerate().map(|(i, (msg, buckets))| {
        let escaped_label = msg.replace('"', "\\\"").replace('\n', " ").chars().take(50).collect::<String>();
        let data_points: String = buckets.iter().map(|(t, c)| {
            format!(r#"{{"t":"{}","c":{}}}"#, t, c)
        }).collect::<Vec<_>>().join(",");
        format!(r#"{{"label":"{}{}","color":"{}","data":[{}]}}"#,
            escaped_label,
            if msg.len() > 50 { "..." } else { "" },
            colors[i % colors.len()],
            data_points
        )
    }).collect::<Vec<_>>().join(",");

    // Build trends table (static, shows totals)
    let trends_table_rows: String = message_trends.iter().enumerate().map(|(i, (msg, buckets))| {
        let total: usize = buckets.iter().map(|(_, c)| *c).sum();
        let escaped_msg = msg.replace('<', "&lt;").replace('>', "&gt;");
        let max_count = buckets.iter().map(|(_, c)| *c).max().unwrap_or(1);
        let sparkline: String = buckets.iter().map(|(_, c)| {
            let height = if max_count > 0 { (*c as f64 / max_count as f64 * 100.0).min(100.0) } else { 0.0 };
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
        .patterns {{ display: flex; flex-wrap: wrap; gap: 15px; margin-bottom: 30px; }}
        .pattern-card {{ background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 15px; flex: 1; min-width: 280px; max-width: 450px; }}
        .pattern-card.critical {{ border-color: #f85149; background: #1a1215; }}
        .pattern-card.warning {{ border-color: #d29922; background: #1a1815; }}
        .pattern-card.info {{ border-color: #58a6ff; }}
        .pattern-header {{ display: flex; align-items: center; gap: 8px; margin-bottom: 8px; }}
        .pattern-header .icon {{ font-size: 1.3em; }}
        .pattern-type {{ font-weight: 600; color: #c9d1d9; font-size: 0.95em; }}
        .pattern-card.critical .pattern-type {{ color: #f85149; }}
        .pattern-card.warning .pattern-type {{ color: #d29922; }}
        .pattern-desc {{ color: #8b949e; font-size: 0.9em; margin: 5px 0; }}
        .pattern-msg {{ color: #6e7681; font-size: 0.8em; margin-top: 8px; padding: 8px; background: #0d1117; border-radius: 4px; word-break: break-all; font-family: monospace; }}
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

        /* Bucket size selector */
        .chart-header {{ display: flex; justify-content: space-between; align-items: center; margin-bottom: 15px; }}
        .chart-title {{ color: #c9d1d9; font-size: 1em; font-weight: 600; }}
        .bucket-selector {{ display: flex; align-items: center; gap: 10px; }}
        .bucket-selector label {{ color: #8b949e; font-size: 0.85em; }}
        .bucket-selector select {{ background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 12px; font-size: 0.85em; cursor: pointer; }}
        .bucket-selector select:hover {{ border-color: #58a6ff; }}
        .bucket-selector select:focus {{ outline: none; border-color: #58a6ff; box-shadow: 0 0 0 2px rgba(88, 166, 255, 0.3); }}
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
                <div class="chart-container full-width">
                    <div class="chart-header">
                        <span class="chart-title">Log Volume Over Time</span>
                        <div class="bucket-selector">
                            <label for="timeBucket">Bucket size:</label>
                            <select id="timeBucket" onchange="updateTimeChart()">
                                <option value="1">1 minute</option>
                                <option value="5">5 minutes</option>
                                <option value="15">15 minutes</option>
                                <option value="30">30 minutes</option>
                                <option value="60" selected>1 hour</option>
                            </select>
                        </div>
                    </div>
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
                    <div class="chart-header">
                        <span class="chart-title">Message Trends</span>
                        <div class="bucket-selector">
                            <label for="trendsBucket">Bucket size:</label>
                            <select id="trendsBucket" onchange="updateTrendsChart()">
                                <option value="1">1 minute</option>
                                <option value="5">5 minutes</option>
                                <option value="15">15 minutes</option>
                                <option value="30">30 minutes</option>
                                <option value="60" selected>1 hour</option>
                            </select>
                        </div>
                    </div>
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
        // Raw minute-level data
        const rawTimeData = [{}];
        const rawTrendsData = [{}];

        // Chart instances
        let timeChart, trendsChart;

        // Tab switching
        function showTab(tabId) {{
            document.querySelectorAll('.tab-content').forEach(el => el.classList.remove('active'));
            document.querySelectorAll('.tab').forEach(el => el.classList.remove('active'));
            document.getElementById(tabId).classList.add('active');
            document.querySelector(`[onclick="showTab('${{tabId}}')"]`).classList.add('active');
        }}

        // Aggregate minute data into larger buckets
        function aggregateData(data, bucketMinutes) {{
            const buckets = {{}};
            data.forEach(d => {{
                const date = new Date(d.t.replace(' ', 'T') + ':00');
                const mins = date.getMinutes();
                const bucketMins = Math.floor(mins / bucketMinutes) * bucketMinutes;
                date.setMinutes(bucketMins, 0, 0);
                const key = date.toISOString().slice(0, 16).replace('T', ' ');
                if (!buckets[key]) buckets[key] = {{ total: 0, errors: 0, warnings: 0 }};
                buckets[key].total += d.total;
                buckets[key].errors += d.errors;
                buckets[key].warnings += d.warnings;
            }});
            return Object.entries(buckets).sort((a, b) => a[0].localeCompare(b[0]));
        }}

        // Aggregate trends data
        function aggregateTrendsData(trendsData, bucketMinutes) {{
            return trendsData.map(series => {{
                const buckets = {{}};
                series.data.forEach(d => {{
                    const date = new Date(d.t.replace(' ', 'T') + ':00');
                    const mins = date.getMinutes();
                    const bucketMins = Math.floor(mins / bucketMinutes) * bucketMinutes;
                    date.setMinutes(bucketMins, 0, 0);
                    const key = date.toISOString().slice(0, 16).replace('T', ' ');
                    buckets[key] = (buckets[key] || 0) + d.c;
                }});
                return {{
                    label: series.label,
                    color: series.color,
                    data: Object.entries(buckets).sort((a, b) => a[0].localeCompare(b[0]))
                }};
            }});
        }}

        // Get all unique bucket keys from trends data
        function getAllTrendBuckets(aggregatedTrends) {{
            const allKeys = new Set();
            aggregatedTrends.forEach(s => s.data.forEach(d => allKeys.add(d[0])));
            return Array.from(allKeys).sort();
        }}

        // Update time series chart
        function updateTimeChart() {{
            const bucketMinutes = parseInt(document.getElementById('timeBucket').value);
            const aggregated = aggregateData(rawTimeData, bucketMinutes);
            const labels = aggregated.map(d => d[0]);
            const totals = aggregated.map(d => d[1].total);
            const errors = aggregated.map(d => d[1].errors);
            const warnings = aggregated.map(d => d[1].warnings);

            timeChart.data.labels = labels;
            timeChart.data.datasets[0].data = totals;
            timeChart.data.datasets[1].data = errors;
            timeChart.data.datasets[2].data = warnings;
            timeChart.update();
        }}

        // Update trends chart
        function updateTrendsChart() {{
            const bucketMinutes = parseInt(document.getElementById('trendsBucket').value);
            const aggregated = aggregateTrendsData(rawTrendsData, bucketMinutes);
            const allBuckets = getAllTrendBuckets(aggregated);

            trendsChart.data.labels = allBuckets;
            trendsChart.data.datasets = aggregated.map(s => ({{
                label: s.label,
                data: allBuckets.map(b => {{
                    const found = s.data.find(d => d[0] === b);
                    return found ? found[1] : 0;
                }}),
                borderColor: s.color,
                backgroundColor: s.color + '22',
                fill: false,
                tension: 0.1
            }}));
            trendsChart.update();
        }}

        // Initialize time chart
        const initTimeData = aggregateData(rawTimeData, 60);
        timeChart = new Chart(document.getElementById('timeChart'), {{
            type: 'line',
            data: {{
                labels: initTimeData.map(d => d[0]),
                datasets: [
                    {{ label: 'Total', data: initTimeData.map(d => d[1].total), borderColor: '#58a6ff', fill: false }},
                    {{ label: 'Errors', data: initTimeData.map(d => d[1].errors), borderColor: '#f85149', fill: false }},
                    {{ label: 'Warnings', data: initTimeData.map(d => d[1].warnings), borderColor: '#d29922', fill: false }}
                ]
            }},
            options: {{
                responsive: true,
                plugins: {{ legend: {{ labels: {{ color: '#c9d1d9' }} }} }},
                scales: {{
                    x: {{ ticks: {{ color: '#8b949e', maxTicksLimit: 12 }}, grid: {{ color: '#30363d' }} }},
                    y: {{ ticks: {{ color: '#8b949e' }}, grid: {{ color: '#30363d' }}, beginAtZero: true }}
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

        // Initialize trends chart
        const initTrendsData = aggregateTrendsData(rawTrendsData, 60);
        const initTrendBuckets = getAllTrendBuckets(initTrendsData);
        trendsChart = new Chart(document.getElementById('trendsChart'), {{
            type: 'line',
            data: {{
                labels: initTrendBuckets,
                datasets: initTrendsData.map(s => ({{
                    label: s.label,
                    data: initTrendBuckets.map(b => {{
                        const found = s.data.find(d => d[0] === b);
                        return found ? found[1] : 0;
                    }}),
                    borderColor: s.color,
                    backgroundColor: s.color + '22',
                    fill: false,
                    tension: 0.1
                }}))
            }},
            options: {{
                responsive: true,
                interaction: {{ mode: 'index', intersect: false }},
                plugins: {{
                    legend: {{
                        position: 'bottom',
                        labels: {{ color: '#c9d1d9', boxWidth: 12, padding: 15 }}
                    }}
                }},
                scales: {{
                    x: {{ ticks: {{ color: '#8b949e', maxTicksLimit: 12 }}, grid: {{ color: '#30363d' }} }},
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
        raw_time_data,
        raw_trends_data,
        priority_labels.join(","),
        priority_counts.join(","),
        service_labels.join(","),
        service_counts.join(",")
    )
}
