// Dashboard HTML generator.
//
// Reads all historical benchmark data from SQLite and produces a single
// self-contained `dashboard.html` file with embedded data and Chart.js charts.
// Open directly in a browser — no server needed.

use super::db::DashboardData;
use std::fs;
use std::path::Path;

/// Generate the dashboard HTML file from benchmark data
pub fn generate(data: &DashboardData, output_path: &Path) {
    let data_json = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());

    let html = build_html(&data_json, data.latest_llm_verdicts.is_empty());

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(output_path, html).expect("Failed to write dashboard.html");
}

/// Build the full dashboard HTML string.
/// All data is embedded as a JS constant so the file is fully self-contained.
fn build_html(data_json: &str, llm_empty: bool) -> String {
    let llm_display = if llm_empty { "none" } else { "block" };

    // The HTML is built in parts to keep each section readable.
    let mut html = String::with_capacity(40_000);

    // --- Head ---
    html.push_str(&format!(r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Essence vs Firecrawl Dashboard</title>
<script src="https://cdn.jsdelivr.net/npm/chart.js@4"></script>
{style}
</head>
<body>
<h1>Essence vs Firecrawl &mdash; Competitive Dashboard</h1>
<p class="subtitle">Auto-generated from benchmark.db &middot; Refresh after each benchmark run</p>
<div id="firecrawlBanner" class="banner warn" style="display:none;">
  <strong>Firecrawl Not Available</strong> &mdash; This run used Essence-only baseline mode. All Firecrawl columns show &ldquo;&mdash;&rdquo; and dimensions like Speed and Markdown Cleanliness are excluded from the radar chart because they aren&rsquo;t comparable without real Firecrawl data. Start Firecrawl (<code>cd firecrawl &amp;&amp; docker compose up -d</code>) and re-run the benchmark for a true head-to-head comparison.
</div>
<div class="grid" id="stats"></div>
<div class="llm-section" style="display:{llm_display}">
<h2>LLM Judge Results (Quality)</h2>
<div class="card" id="llmResults"></div>
</div>
<h2>Win Rate Over Time</h2>
<div class="card"><div class="chart-box"><canvas id="winRateChart"></canvas></div></div>
<h2>Per-Category Win Rates (Heuristic)</h2>
<div class="card"><div class="chart-box"><canvas id="categoryChart"></canvas></div></div>
<h2>Per-Dimension Win Rates (Latest Run)</h2>
<div class="card"><div class="chart-box"><canvas id="dimensionRadar"></canvas></div></div>
<h2>Speed Trend</h2>
<div class="card"><div class="chart-box"><canvas id="speedTrendChart"></canvas></div></div>
<h2>Dimension Win Rate Trends</h2>
<div class="card"><div class="chart-box chart-box-tall"><canvas id="dimensionTrendChart"></canvas></div></div>
<h2>Category Win Rate Trends</h2>
<div class="card"><div class="chart-box chart-box-tall"><canvas id="categoryTrendChart"></canvas></div></div>
<h2>Content Metric Trends</h2>
<div class="card"><div class="chart-box"><canvas id="metricTrendChart"></canvas></div></div>
<h2>URL-Level Results (Latest Run)</h2>
<div class="scroll-table card" style="padding:0;">
<table id="urlTable"><thead><tr>
<th>Category</th><th>URL</th><th>Quality</th><th>Speed</th><th>Heuristic</th><th>Advantage</th>
<th>E Words</th><th>F Words</th><th>E Speed</th><th>F Speed</th>
</tr></thead><tbody></tbody></table>
</div>
<h2>Run History</h2>
<div class="scroll-table card" style="padding:0;">
<table id="runTable"><thead><tr>
<th>#</th><th>Time</th><th>Commit</th><th>URLs</th>
<th>Quality</th><th>Speed</th><th>Heuristic</th>
</tr></thead><tbody></tbody></table>
</div>
"##,
        style = CSS,
        llm_display = llm_display,
    ));

    // --- Embedded data + JS ---
    html.push_str("<script>\nconst DATA = ");
    html.push_str(data_json);
    html.push_str(";\n");
    html.push_str(DASHBOARD_JS);
    html.push_str("\n</script>\n</body>\n</html>");

    html
}

// MARK: - Embedded CSS

const CSS: &str = r#"<style>
:root {
    --bg:#0d1117; --surface:#161b22; --border:#30363d;
    --text:#e6edf3; --muted:#8b949e; --accent:#58a6ff;
    --green:#3fb950; --red:#f85149; --yellow:#d29922;
    --purple:#bc8cff;
}
* { margin:0; padding:0; box-sizing:border-box; }
body { background:var(--bg); color:var(--text); font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif; padding:24px; }
h1 { font-size:1.6rem; margin-bottom:4px; }
.subtitle { color:var(--muted); font-size:.9rem; margin-bottom:16px; }
h2 { font-size:1.15rem; color:var(--muted); margin:32px 0 12px; border-bottom:1px solid var(--border); padding-bottom:6px; }
.grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(180px,1fr)); gap:12px; margin:16px 0; }
.card { background:var(--surface); border:1px solid var(--border); border-radius:8px; padding:20px; }
.stat { font-size:2.2rem; font-weight:700; line-height:1.1; }
.stat.green { color:var(--green); } .stat.red { color:var(--red); } .stat.yellow { color:var(--yellow); } .stat.purple { color:var(--purple); }
.label { color:var(--muted); font-size:.85rem; margin-top:4px; }
.chart-box { position:relative; height:340px; }
.chart-box-tall { height:420px; }
canvas { max-height:420px; }
table { width:100%; border-collapse:collapse; font-size:.85rem; margin-top:0; }
th,td { padding:8px 10px; text-align:left; border-bottom:1px solid var(--border); }
th { color:var(--muted); font-weight:600; position:sticky; top:0; background:var(--surface); }
.badge { display:inline-block; padding:2px 8px; border-radius:4px; font-size:.75rem; font-weight:600; }
.badge.essence { background:#16371e; color:var(--green); }
.badge.firecrawl { background:#3d1519; color:var(--red); }
.badge.tie { background:#2e2a15; color:var(--yellow); }
.badge.pending { background:#1c1c2e; color:var(--muted); }
.scroll-table { max-height:500px; overflow-y:auto; border:1px solid var(--border); border-radius:8px; }
.empty-msg { color:var(--muted); text-align:center; padding:40px; font-style:italic; }
.banner { border-radius:8px; padding:14px 20px; margin-bottom:16px; font-size:.9rem; line-height:1.5; }
.banner.warn { background:#2e2a15; border:1px solid var(--yellow); color:var(--yellow); }
.banner code { background:rgba(255,255,255,0.08); padding:2px 6px; border-radius:3px; font-size:.82rem; }
.chart-note { color:var(--muted); font-size:.8rem; font-style:italic; margin-top:8px; text-align:center; }
.na { color:var(--muted); }
</style>"#;

// MARK: - Embedded JavaScript

const DASHBOARD_JS: &str = r#"
// Human-readable labels for dimension keys
var DIMENSION_LABELS = {
    'word_count': 'Word Count',
    'heading_preservation': 'Heading Preservation',
    'link_preservation': 'Link Preservation',
    'image_preservation': 'Image Preservation',
    'code_block_preservation': 'Code Block Preservation',
    'markdown_cleanliness': 'Markdown Cleanliness',
    'content_density': 'Content Density',
    'metadata_extraction': 'Metadata Extraction',
    'speed': 'Speed'
};

// Human-readable labels for category keys
var CATEGORY_LABELS = {
    'content': 'Long-Form Content',
    'docs': 'Documentation',
    'dynamic': 'Dynamic / JS-Heavy',
    'structured': 'Structured Data',
    'news': 'News & Media',
    'ecommerce': 'E-Commerce',
    'reference': 'Reference & Community'
};

function dimLabel(key) { return DIMENSION_LABELS[key] || key.replace(/_/g, ' ').replace(/\b\w/g, function(c){ return c.toUpperCase(); }); }
function catLabel(key) { return CATEGORY_LABELS[key] || key.replace(/_/g, ' ').replace(/\b\w/g, function(c){ return c.toUpperCase(); }); }

function makeBadge(winner) {
    if (!winner || winner === 'pending') return '<span class="badge pending">Pending</span>';
    var label = winner.replace(/\b\w/g, function(c){ return c.toUpperCase(); });
    return '<span class="badge ' + winner + '">' + label + '</span>';
}

// Detect whether Firecrawl data is present
function hasFirecrawlData() {
    var results = DATA.latest_url_results || [];
    return results.some(function(r){ return r.f_success || r.f_word_count > 0; });
}

// Helper: set element content safely
function setHTML(id, content) {
    var el = document.getElementById(id);
    if (el) { el.replaceChildren(); el.insertAdjacentHTML('afterbegin', content); }
}
function appendRows(tableId, rowsHtml) {
    var tbody = document.querySelector('#' + tableId + ' tbody');
    if (tbody) { tbody.insertAdjacentHTML('beforeend', rowsHtml); }
}

// --- Firecrawl Status Banner ---
(function() {
    var banner = document.getElementById('firecrawlBanner');
    if (!banner) return;
    if (!hasFirecrawlData()) {
        banner.style.display = 'block';
    }
})();

// --- Header Stats (Two Leaderboards) ---
(function() {
    var runs = DATA.runs || [];
    var latest = runs[runs.length - 1];
    if (!latest) { setHTML('stats', '<div class="empty-msg">No benchmark runs yet.</div>'); return; }
    var fcPresent = hasFirecrawlData();
    var llm = DATA.latest_llm_verdicts || [];
    var nUrls = (DATA.latest_url_results || []).length;
    var modeLabel = fcPresent ? 'Head-to-Head' : 'Essence Only (Baseline)';

    // Quality Win Rate (LLM Judge) — primary metric
    var qPct = (latest.quality_win_rate * 100).toFixed(1);
    var qCls = latest.quality_win_rate >= 0.9 ? 'green' : latest.quality_win_rate >= 0.5 ? 'yellow' : 'red';
    var qLabel = llm.length > 0 ? qPct + '%' : '\u2014';
    var qClsFinal = llm.length > 0 ? qCls : '';

    // Speed Win Rate
    var sPct = (latest.speed_win_rate * 100).toFixed(1);
    var sCls = latest.speed_win_rate >= 0.9 ? 'green' : latest.speed_win_rate >= 0.5 ? 'yellow' : 'red';

    // Heuristic Win Rate (artifact counting — diagnostic)
    var hPct = (latest.essence_win_rate * 100).toFixed(1);
    var hCls = latest.essence_win_rate >= 0.9 ? 'green' : latest.essence_win_rate >= 0.5 ? 'yellow' : 'red';

    setHTML('stats',
        '<div class="card"><div class="stat ' + qClsFinal + '">' + qLabel + '</div><div class="label">Quality Win Rate (LLM Judge)</div></div>' +
        '<div class="card"><div class="stat ' + sCls + '">' + sPct + '%</div><div class="label">Speed Win Rate</div></div>' +
        '<div class="card"><div class="stat ' + hCls + '">' + hPct + '%</div><div class="label">Heuristic Win Rate</div></div>' +
        '<div class="card"><div class="stat">' + nUrls + '</div><div class="label">URLs Tested</div></div>' +
        '<div class="card"><div class="stat">' + (llm.length > 0 ? llm.length : '\u2014') + '</div><div class="label">LLM Verdicts</div></div>' +
        '<div class="card"><div class="stat" style="font-size:1.1rem;">' + modeLabel + '</div><div class="label">' + (latest.git_commit || '\u2014') + ' \u00b7 ' + latest.timestamp.slice(0,16).replace('T',' ') + '</div></div>'
    );
})();

// --- LLM Judge Results (promoted to top) ---
(function() {
    var verdicts = DATA.latest_llm_verdicts || [];
    if (verdicts.length === 0) { setHTML('llmResults', '<div class="empty-msg">No LLM verdicts yet. LLM judge is on by default — run benchmark to populate.</div>'); return; }
    var dims = ['content_relevance','noise_removal','readability','structural_coherence','information_completeness','token_efficiency','overall_llm'];
    var llmLabels = {
        'content_relevance': 'Content Relevance',
        'noise_removal': 'Noise Removal',
        'readability': 'Readability',
        'structural_coherence': 'Structural Coherence',
        'information_completeness': 'Information Completeness',
        'token_efficiency': 'Token Efficiency',
        'overall_llm': 'Overall (LLM)'
    };
    var counts = {};
    dims.forEach(function(d){ counts[d] = {essence:0, firecrawl:0, tie:0}; });
    verdicts.forEach(function(v){
        dims.forEach(function(d){
            var w = v[d + '_winner'] || 'tie';
            if (counts[d][w] !== undefined) counts[d][w]++;
        });
    });
    var h = '<table><thead><tr><th>Dimension</th><th>Essence</th><th>Firecrawl</th><th>Tie</th><th>Essence Rate</th></tr></thead><tbody>';
    dims.forEach(function(d){
        var total = verdicts.length;
        var rate = ((counts[d].essence / total) * 100).toFixed(0);
        var isOverall = d === 'overall_llm';
        var style = isOverall ? ' style="font-weight:600;border-top:2px solid var(--border);"' : '';
        h += '<tr' + style + '><td>' + (llmLabels[d] || d) + '</td><td>' + counts[d].essence + '</td><td>' + counts[d].firecrawl + '</td><td>' + counts[d].tie + '</td><td>' + rate + '%</td></tr>';
    });
    h += '</tbody></table>';
    setHTML('llmResults', h);
})();

// --- Win Rate Over Time (Quality + Heuristic + Speed) ---
(function() {
    var runs = DATA.runs || [];
    if (runs.length === 0) return;
    var labels = runs.map(function(r){ return 'Run #' + r.id; });

    var datasets = [{
        label: 'Heuristic Win Rate',
        data: runs.map(function(r){ return (r.essence_win_rate * 100).toFixed(1); }),
        borderColor: '#58a6ff', backgroundColor: 'rgba(88,166,255,0.1)',
        fill: false, tension: 0.3
    }];

    // Quality trend from LLM judge
    var qt = DATA.quality_trend || [];
    if (qt.length > 0) {
        // Map quality trend by run_id for alignment
        var qMap = {};
        qt.forEach(function(p){ qMap[p[0]] = p[1]; });
        datasets.unshift({
            label: 'Quality Win Rate (LLM)',
            data: runs.map(function(r){ return qMap[r.id] !== undefined ? (qMap[r.id] * 100).toFixed(1) : null; }),
            borderColor: '#bc8cff', backgroundColor: 'rgba(188,140,255,0.1)',
            fill: false, tension: 0.3, borderWidth: 3
        });
    }

    // Speed win rate from runs
    var hasSpeed = runs.some(function(r){ return r.speed_win_rate > 0; });
    if (hasSpeed) {
        datasets.push({
            label: 'Speed Win Rate',
            data: runs.map(function(r){ return (r.speed_win_rate * 100).toFixed(1); }),
            borderColor: '#39d2c0', backgroundColor: 'rgba(57,210,192,0.1)',
            fill: false, tension: 0.3, borderDash: [5,3]
        });
    }

    datasets.push({
        label: '90% Target',
        data: runs.map(function(){ return 90; }),
        borderColor: '#3fb950', borderDash: [5,5], pointRadius: 0
    });

    new Chart(document.getElementById('winRateChart'), {
        type: 'line',
        data: { labels: labels, datasets: datasets },
        options: { responsive:true, maintainAspectRatio:false,
            scales: { y: { min:0, max:100, ticks:{ color:'#8b949e' }, grid:{ color:'#30363d' } },
                       x: { ticks:{ color:'#8b949e' }, grid:{ color:'#30363d' } } },
            plugins: { legend: { labels: { color:'#e6edf3' } } }
        }
    });
})();

// --- Speed Trend ---
(function() {
    var trends = DATA.speed_trends || [];
    if (trends.length < 2) return;
    new Chart(document.getElementById('speedTrendChart'), {
        type: 'line',
        data: {
            labels: trends.map(function(t){ return 'Run #' + t.run_id; }),
            datasets: [{
                label: 'Essence Avg (ms)',
                data: trends.map(function(t){ return t.avg_essence_ms.toFixed(0); }),
                borderColor: '#58a6ff', backgroundColor: 'rgba(88,166,255,0.1)',
                fill: false, tension: 0.3
            },{
                label: 'Firecrawl Avg (ms)',
                data: trends.map(function(t){ return t.avg_firecrawl_ms.toFixed(0); }),
                borderColor: '#f85149', backgroundColor: 'rgba(248,81,73,0.1)',
                fill: false, tension: 0.3
            }]
        },
        options: { responsive:true, maintainAspectRatio:false,
            scales: { y: { beginAtZero:true, ticks:{ color:'#8b949e', callback:function(v){return v+'ms';} }, grid:{ color:'#30363d' } },
                       x: { ticks:{ color:'#8b949e' }, grid:{ color:'#30363d' } } },
            plugins: { legend: { labels: { color:'#e6edf3' } },
                       tooltip: { callbacks: { label: function(ctx){ return ctx.dataset.label + ': ' + ctx.parsed.y + 'ms'; } } } }
        }
    });
})();

// --- Dimension Win Rate Trends ---
(function() {
    var dt = DATA.dimension_trends || {};
    var keys = Object.keys(dt).sort();
    if (keys.length === 0) return;
    var firstKey = keys[0];
    if ((dt[firstKey] || []).length < 2) return;

    var palette = ['#58a6ff','#f85149','#3fb950','#d29922','#bc8cff','#f0883e','#39d2c0','#ff7eb6','#79c0ff'];
    var datasets = keys.map(function(k, i) {
        var pts = dt[k] || [];
        return {
            label: dimLabel(k),
            data: pts.map(function(p){ return (p[1]*100).toFixed(1); }),
            borderColor: palette[i % palette.length],
            backgroundColor: 'transparent',
            tension: 0.3,
            borderWidth: 2,
            pointRadius: 3
        };
    });
    var labels = (dt[firstKey] || []).map(function(p){ return 'Run #' + p[0]; });

    new Chart(document.getElementById('dimensionTrendChart'), {
        type: 'line',
        data: { labels: labels, datasets: datasets },
        options: { responsive:true, maintainAspectRatio:false,
            scales: { y: { min:0, max:100, ticks:{ color:'#8b949e', callback:function(v){return v+'%';} }, grid:{ color:'#30363d' } },
                       x: { ticks:{ color:'#8b949e' }, grid:{ color:'#30363d' } } },
            plugins: { legend: { labels: { color:'#e6edf3', font:{ size:11 } }, position:'bottom' },
                       tooltip: { callbacks: { label: function(ctx){ return ctx.dataset.label + ': ' + ctx.parsed.y + '%'; } } } }
        }
    });
})();

// --- Category Win Rate Trends ---
(function() {
    var ct = DATA.category_trends || {};
    var keys = Object.keys(ct).sort();
    if (keys.length === 0) return;
    var firstKey = keys[0];
    if ((ct[firstKey] || []).length < 2) return;

    var palette = ['#58a6ff','#f85149','#3fb950','#d29922','#bc8cff','#f0883e','#39d2c0','#ff7eb6'];
    var datasets = keys.map(function(k, i) {
        var pts = ct[k] || [];
        return {
            label: catLabel(k),
            data: pts.map(function(p){ return (p[1]*100).toFixed(1); }),
            borderColor: palette[i % palette.length],
            backgroundColor: 'transparent',
            tension: 0.3,
            borderWidth: 2,
            pointRadius: 3
        };
    });
    var labels = (ct[firstKey] || []).map(function(p){ return 'Run #' + p[0]; });

    new Chart(document.getElementById('categoryTrendChart'), {
        type: 'line',
        data: { labels: labels, datasets: datasets },
        options: { responsive:true, maintainAspectRatio:false,
            scales: { y: { min:0, max:100, ticks:{ color:'#8b949e', callback:function(v){return v+'%';} }, grid:{ color:'#30363d' } },
                       x: { ticks:{ color:'#8b949e' }, grid:{ color:'#30363d' } } },
            plugins: { legend: { labels: { color:'#e6edf3', font:{ size:11 } }, position:'bottom' },
                       tooltip: { callbacks: { label: function(ctx){ return ctx.dataset.label + ': ' + ctx.parsed.y + '%'; } } } }
        }
    });
})();

// --- Content Metric Trends ---
(function() {
    var mt = DATA.metric_trends || [];
    if (mt.length < 2) return;
    var labels = mt.map(function(t){ return 'Run #' + t.run_id; });
    new Chart(document.getElementById('metricTrendChart'), {
        type: 'line',
        data: {
            labels: labels,
            datasets: [{
                label: 'Essence Avg Words',
                data: mt.map(function(t){ return t.avg_e_word_count.toFixed(0); }),
                borderColor: '#58a6ff', backgroundColor: 'transparent',
                tension: 0.3, borderWidth: 2, yAxisID: 'y'
            },{
                label: 'Firecrawl Avg Words',
                data: mt.map(function(t){ return t.avg_f_word_count.toFixed(0); }),
                borderColor: '#f85149', backgroundColor: 'transparent',
                tension: 0.3, borderWidth: 2, yAxisID: 'y'
            },{
                label: 'Essence Avg Headings',
                data: mt.map(function(t){ return t.avg_e_headings.toFixed(1); }),
                borderColor: '#3fb950', backgroundColor: 'transparent',
                tension: 0.3, borderWidth: 2, borderDash: [5,3], yAxisID: 'y1'
            },{
                label: 'Firecrawl Avg Headings',
                data: mt.map(function(t){ return t.avg_f_headings.toFixed(1); }),
                borderColor: '#d29922', backgroundColor: 'transparent',
                tension: 0.3, borderWidth: 2, borderDash: [5,3], yAxisID: 'y1'
            }]
        },
        options: { responsive:true, maintainAspectRatio:false,
            scales: {
                y: { type:'linear', position:'left', beginAtZero:true,
                     title:{ display:true, text:'Avg Word Count', color:'#8b949e' },
                     ticks:{ color:'#8b949e' }, grid:{ color:'#30363d' } },
                y1: { type:'linear', position:'right', beginAtZero:true,
                      title:{ display:true, text:'Avg Headings', color:'#8b949e' },
                      ticks:{ color:'#8b949e' }, grid:{ drawOnChartArea:false } },
                x: { ticks:{ color:'#8b949e' }, grid:{ color:'#30363d' } }
            },
            plugins: { legend: { labels: { color:'#e6edf3', font:{ size:11 } }, position:'bottom' } }
        }
    });
})();

// --- Per-Category Bar ---
(function() {
    var cats = DATA.category_win_rates || {};
    var keys = Object.keys(cats).sort();
    if (keys.length === 0) return;
    new Chart(document.getElementById('categoryChart'), {
        type: 'bar',
        data: {
            labels: keys.map(catLabel),
            datasets: [{
                label: 'Win Rate %',
                data: keys.map(function(k){ return (cats[k]*100).toFixed(1); }),
                backgroundColor: keys.map(function(k){ return cats[k]>=0.8?'#3fb950':cats[k]>=0.5?'#d29922':'#f85149'; }),
                borderRadius: 4
            }]
        },
        options: { responsive:true, maintainAspectRatio:false,
            scales: { y: { min:0, max:100, ticks:{ color:'#8b949e' }, grid:{ color:'#30363d' } },
                       x: { ticks:{ color:'#8b949e' }, grid:{ display:false } } },
            plugins: { legend: { display:false } }
        }
    });
})();

// --- Dimension Radar (Essence vs Firecrawl) ---
(function() {
    var results = DATA.latest_url_results || [];
    if (results.length === 0) return;
    var fcPresent = hasFirecrawlData();

    var dimKeys = [
        'word_count', 'heading_preservation', 'link_preservation',
        'image_preservation', 'code_block_preservation',
        'markdown_cleanliness', 'content_density', 'metadata_extraction', 'speed'
    ];

    var essenceRates = [];
    var firecrawlRates = [];
    var filteredDims = [];

    dimKeys.forEach(function(d) {
        if (!fcPresent && (d === 'speed' || d === 'markdown_cleanliness')) return;
        var eWins = 0, fWins = 0, total = results.length;
        results.forEach(function(r) {
            var w = r['w_' + d] || 'tie';
            if (w === 'essence') eWins++;
            else if (w === 'firecrawl') fWins++;
        });
        filteredDims.push(d);
        essenceRates.push((eWins / total * 100).toFixed(1));
        firecrawlRates.push((fWins / total * 100).toFixed(1));
    });

    if (filteredDims.length === 0) return;
    new Chart(document.getElementById('dimensionRadar'), {
        type: 'radar',
        data: {
            labels: filteredDims.map(dimLabel),
            datasets: [{
                label: 'Essence', data: essenceRates,
                borderColor:'#58a6ff', backgroundColor:'rgba(88,166,255,0.12)',
                pointBackgroundColor:'#58a6ff', borderWidth: 2
            },{
                label: 'Firecrawl', data: firecrawlRates,
                borderColor:'#f85149', backgroundColor:'rgba(248,81,73,0.08)',
                pointBackgroundColor:'#f85149', borderWidth: 2
            },{
                label: '70% Target', data: filteredDims.map(function(){return 70;}),
                borderColor:'#3fb950', borderDash:[3,3], pointRadius:0, backgroundColor:'transparent'
            }]
        },
        options: { responsive:true, maintainAspectRatio:false,
            scales: { r: { min:0, max:100, ticks:{ color:'#8b949e', backdropColor:'transparent' },
                            grid:{ color:'#30363d' }, pointLabels:{ color:'#e6edf3', font:{ size:12 } } } },
            plugins: { legend: { labels: { color:'#e6edf3' } } }
        }
    });
    if (!fcPresent) {
        var canvas = document.getElementById('dimensionRadar');
        var note = document.createElement('p');
        note.className = 'chart-note';
        note.textContent = 'Speed and Markdown Cleanliness are excluded (not comparable without Firecrawl data).';
        canvas.parentNode.appendChild(note);
    }
})();

// --- URL Detail (Quality + Speed + Heuristic columns) ---
(function() {
    var results = DATA.latest_url_results || [];
    var fcPresent = hasFirecrawlData();
    var rows = '';
    results.forEach(function(r){
        var short = r.url.replace('https://','').replace('http://','').slice(0,45);
        rows += '<tr><td>' + catLabel(r.category) + '</td>';
        rows += '<td title="' + r.url + '" style="max-width:280px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">' + short + '</td>';
        rows += '<td>' + makeBadge(r.quality_winner) + '</td>';
        rows += '<td>' + makeBadge(r.speed_winner) + '</td>';
        rows += '<td>' + makeBadge(r.overall_winner) + '</td>';
        rows += '<td>' + r.essence_advantage.toFixed(2) + '</td>';
        rows += '<td>' + r.e_word_count.toLocaleString() + '</td>';
        rows += '<td>' + (fcPresent ? r.f_word_count.toLocaleString() : '<span class="na">\u2014</span>') + '</td>';
        rows += '<td>' + r.e_response_time_ms + 'ms</td>';
        rows += '<td>' + (fcPresent ? r.f_response_time_ms + 'ms' : '<span class="na">\u2014</span>') + '</td>';
        rows += '</tr>';
    });
    appendRows('urlTable', rows);
})();

// --- Run History (Quality + Speed + Heuristic) ---
(function() {
    var runs = (DATA.runs || []).slice().reverse();
    var rows = '';
    runs.forEach(function(r){
        var qPct = (r.quality_win_rate * 100).toFixed(1);
        var qCls = r.quality_win_rate >= 0.9 ? 'green' : r.quality_win_rate >= 0.5 ? 'yellow' : 'red';
        var sPct = (r.speed_win_rate * 100).toFixed(1);
        var sCls = r.speed_win_rate >= 0.9 ? 'green' : r.speed_win_rate >= 0.5 ? 'yellow' : 'red';
        var hPct = (r.essence_win_rate * 100).toFixed(1);
        var hCls = r.essence_win_rate >= 0.9 ? 'green' : r.essence_win_rate >= 0.5 ? 'yellow' : 'red';
        var hasQuality = r.quality_win_rate > 0;
        rows += '<tr><td>#' + r.id + '</td>';
        rows += '<td>' + r.timestamp.slice(0,16).replace('T',' ') + '</td>';
        rows += '<td><code>' + r.git_commit + '</code></td>';
        rows += '<td>' + r.total_urls + '</td>';
        rows += '<td style="color:var(--' + qCls + ');font-weight:600;">' + (hasQuality ? qPct + '%' : '\u2014') + '</td>';
        rows += '<td style="color:var(--' + sCls + ');font-weight:600;">' + sPct + '%</td>';
        rows += '<td style="color:var(--' + hCls + ');font-weight:600;">' + hPct + '%</td>';
        rows += '</tr>';
    });
    appendRows('runTable', rows);
})();
"#;
