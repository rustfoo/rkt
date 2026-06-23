#!/usr/bin/env python3
"""Read oha JSON results and emit a self-contained HTML chart page."""

import json
import os
import sys

RESULTS_DIR = os.path.join(os.path.dirname(__file__), "results")
OUT_FILE = os.path.join(os.path.dirname(__file__), "results.html")

FRAMEWORKS = ["rkt", "axum", "actix"]
COLORS = {
    "rkt":   {"bg": "rgba(220, 80,  60,  0.75)", "border": "rgba(220, 80,  60,  1)"},
    "axum":  {"bg": "rgba(60,  130, 220, 0.75)", "border": "rgba(60,  130, 220, 1)"},
    "actix": {"bg": "rgba(60,  180, 100, 0.75)", "border": "rgba(60,  180, 100, 1)"},
}

API_SCENARIOS    = ["ping", "hello", "query", "headers", "state"]
FILE_SCENARIOS   = ["file-small", "file-large"]

def load(framework, scenario):
    path = os.path.join(RESULTS_DIR, f"{framework}-{scenario}.json")
    if not os.path.exists(path):
        return None
    with open(path) as f:
        return json.load(f)

def rps(data):
    return round(data["summary"]["requestsPerSec"]) if data else None

def p99_ms(data):
    return round(data["latencyPercentiles"]["p99"] * 1000, 2) if data else None

def p50_ms(data):
    return round(data["latencyPercentiles"]["p50"] * 1000, 2) if data else None

def js_array(values):
    return "[" + ", ".join(str(v) if v is not None else "null" for v in values) + "]"

def dataset(label, scenarios, metric_fn):
    fw = label
    values = [metric_fn(load(fw, s)) for s in scenarios]
    c = COLORS[fw]
    return f"""{{
      label: '{fw}',
      data: {js_array(values)},
      backgroundColor: '{c["bg"]}',
      borderColor: '{c["border"]}',
      borderWidth: 1.5,
      borderRadius: 3
    }}"""

def chart(canvas_id, title, scenarios, metric_fn, y_label):
    labels = json.dumps(scenarios)
    datasets = ",\n      ".join(dataset(fw, scenarios, metric_fn) for fw in FRAMEWORKS)
    return f"""
    new Chart(document.getElementById('{canvas_id}'), {{
      type: 'bar',
      data: {{
        labels: {labels},
        datasets: [
          {datasets}
        ]
      }},
      options: {{
        responsive: true,
        plugins: {{
          title: {{ display: true, text: '{title}', font: {{ size: 14, weight: 'bold' }} }},
          legend: {{ position: 'top' }}
        }},
        scales: {{
          y: {{
            beginAtZero: true,
            title: {{ display: true, text: '{y_label}' }}
          }}
        }}
      }}
    }});"""

html = f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>rkt vs axum vs actix-web — Benchmarks</title>
  <script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.0/dist/chart.umd.min.js"></script>
  <style>
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{ font-family: system-ui, sans-serif; background: #f7f7f7; color: #222; padding: 2rem; }}
    h1 {{ font-size: 1.4rem; margin-bottom: 0.4rem; }}
    p.subtitle {{ color: #666; font-size: 0.9rem; margin-bottom: 2rem; }}
    .grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 2rem; }}
    .grid.wide {{ grid-template-columns: 1fr; max-width: 700px; }}
    .card {{ background: #fff; border-radius: 8px; padding: 1.5rem; box-shadow: 0 1px 4px rgba(0,0,0,.08); }}
    h2 {{ font-size: 1rem; color: #555; margin-bottom: 1.2rem; border-bottom: 1px solid #eee; padding-bottom: 0.5rem; }}
    canvas {{ max-height: 340px; }}
    .note {{ margin-top: 2rem; font-size: 0.8rem; color: #888; }}
  </style>
</head>
<body>
  <h1>rkt vs axum vs actix-web — HTTP Benchmark Results</h1>
  <p class="subtitle">Sequential oha runs · 100 concurrent connections · 30 s per scenario</p>

  <h2 style="margin-bottom:1rem">Throughput — API Scenarios</h2>
  <div class="grid">
    <div class="card"><canvas id="api_rps"></canvas></div>
    <div class="card"><canvas id="api_p99"></canvas></div>
  </div>

  <h2 style="margin-top:2rem;margin-bottom:1rem">Throughput — File Serving</h2>
  <div class="grid">
    <div class="card"><canvas id="file_rps"></canvas></div>
    <div class="card"><canvas id="file_p99"></canvas></div>
  </div>

  <p class="note">
    Generated from raw JSON in <code>results/</code>.
    Lower p99 is better. Higher req/s is better.
  </p>

  <script>
    {chart("api_rps",  "Requests / second",   API_SCENARIOS,  rps,    "req/s")}
    {chart("api_p99",  "P99 Latency (ms)",     API_SCENARIOS,  p99_ms, "ms")}
    {chart("file_rps", "Requests / second",    FILE_SCENARIOS, rps,    "req/s")}
    {chart("file_p99", "P99 Latency (ms)",     FILE_SCENARIOS, p99_ms, "ms")}
  </script>
</body>
</html>
"""

# Substitute the Python lambda names with actual JS-compatible values by
# rendering everything server-side in Python rather than passing callables.

# Re-generate properly — the chart() calls above passed Python callables
# as strings; redo with concrete data.

def chart_rendered(canvas_id, title, scenarios, metric_fn, y_label):
    labels = json.dumps(scenarios)
    ds_parts = []
    for fw in FRAMEWORKS:
        values = [metric_fn(load(fw, s)) for s in scenarios]
        c = COLORS[fw]
        ds_parts.append(f"""{{
          label: '{fw}',
          data: {js_array(values)},
          backgroundColor: '{c["bg"]}',
          borderColor: '{c["border"]}',
          borderWidth: 1.5,
          borderRadius: 3
        }}""")
    datasets = ",\n      ".join(ds_parts)
    return f"""new Chart(document.getElementById('{canvas_id}'), {{
      type: 'bar',
      data: {{
        labels: {labels},
        datasets: [{datasets}]
      }},
      options: {{
        responsive: true,
        plugins: {{
          title: {{ display: true, text: '{title}', font: {{ size: 14, weight: 'bold' }} }},
          legend: {{ position: 'top' }}
        }},
        scales: {{
          y: {{
            beginAtZero: true,
            title: {{ display: true, text: '{y_label}' }}
          }}
        }}
      }}
    }});"""

html = f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>rkt vs axum vs actix-web — Benchmarks</title>
  <script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.0/dist/chart.umd.min.js"></script>
  <style>
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{ font-family: system-ui, sans-serif; background: #f7f7f7; color: #222; padding: 2rem; }}
    h1 {{ font-size: 1.5rem; margin-bottom: 0.3rem; }}
    p.subtitle {{ color: #666; font-size: 0.88rem; margin-bottom: 2.5rem; }}
    .section-title {{ font-size: 1rem; font-weight: 600; color: #444;
                      margin: 2rem 0 1rem; border-left: 3px solid #ccc; padding-left: 0.6rem; }}
    .grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 1.5rem; max-width: 1100px; }}
    .card {{ background: #fff; border-radius: 8px; padding: 1.5rem;
             box-shadow: 0 1px 4px rgba(0,0,0,.08); }}
    canvas {{ max-height: 320px; }}
    .note {{ margin-top: 2rem; font-size: 0.78rem; color: #999; max-width: 1100px; }}
  </style>
</head>
<body>
  <h1>rkt vs axum vs actix-web</h1>
  <p class="subtitle">Sequential oha runs &middot; 100 concurrent connections &middot; 30 s per scenario</p>

  <div class="section-title">API Scenarios</div>
  <div class="grid">
    <div class="card"><canvas id="api_rps"></canvas></div>
    <div class="card"><canvas id="api_p99"></canvas></div>
  </div>

  <div class="section-title">File Serving</div>
  <div class="grid">
    <div class="card"><canvas id="file_rps"></canvas></div>
    <div class="card"><canvas id="file_p99"></canvas></div>
  </div>

  <p class="note">
    Generated from <code>results/*.json</code> &middot;
    Higher req/s is better &middot; Lower p99 is better
  </p>

  <script>
    {chart_rendered("api_rps",  "Requests / second",  API_SCENARIOS,  rps,    "req/s")}
    {chart_rendered("api_p99",  "P99 Latency (ms)",   API_SCENARIOS,  p99_ms, "ms")}
    {chart_rendered("file_rps", "Requests / second",  FILE_SCENARIOS, rps,    "req/s")}
    {chart_rendered("file_p99", "P99 Latency (ms)",   FILE_SCENARIOS, p99_ms, "ms")}
  </script>
</body>
</html>
"""

with open(OUT_FILE, "w") as f:
    f.write(html)

print(f"Written: {OUT_FILE}")
