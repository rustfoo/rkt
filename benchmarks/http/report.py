#!/usr/bin/env python3
"""Aggregate one HTTP benchmark run into JSON, terminal, and HTML reports."""

from __future__ import annotations

import argparse
import html
import json
import statistics
from collections import defaultdict
from pathlib import Path
from typing import Any, Callable


FRAMEWORKS = ("rkt", "axum", "actix")


def median(values: list[float]) -> float:
    return statistics.median(values)


def mad(values: list[float]) -> float:
    center = median(values)
    return median([abs(value - center) for value in values])


def aggregate(runs: list[dict[str, Any]]) -> dict[str, Any]:
    def values(getter: Callable[[dict[str, Any]], float]) -> list[float]:
        return [getter(run) for run in runs]

    rps = values(lambda run: run["summary"]["requestsPerSec"])
    bytes_per_second = values(lambda run: run["summary"]["sizePerSec"])
    p50 = values(lambda run: run["latencyPercentiles"]["p50"] * 1000)
    p95 = values(lambda run: run["latencyPercentiles"]["p95"] * 1000)
    p99 = values(lambda run: run["latencyPercentiles"]["p99"] * 1000)
    first_byte_p50 = values(lambda run: run["firstBytePercentiles"]["p50"] * 1000)
    first_byte_p99 = values(lambda run: run["firstBytePercentiles"]["p99"] * 1000)
    cpu = values(lambda run: run["benchmark"]["serverCpuPercent"])
    rss = values(lambda run: run["benchmark"]["serverPeakRssKiB"] / 1024)
    return {
        "samples": len(runs),
        "requestsPerSecond": {"median": median(rps), "mad": mad(rps)},
        "bytesPerSecond": {"median": median(bytes_per_second), "mad": mad(bytes_per_second)},
        "latencyMs": {
            "p50Median": median(p50),
            "p95Median": median(p95),
            "p99Median": median(p99),
        },
        "firstByteMs": {
            "p50Median": median(first_byte_p50),
            "p99Median": median(first_byte_p99),
        },
        "serverCpuPercent": {"median": median(cpu), "mad": mad(cpu)},
        "serverPeakRssMiB": {"median": median(rss), "max": max(rss)},
    }


def load_run(run_dir: Path) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    metadata_path = run_dir / "metadata.json"
    if not metadata_path.is_file():
        raise SystemExit(f"missing benchmark metadata: {metadata_path}")
    metadata = json.loads(metadata_path.read_text())

    runs = []
    for path in sorted((run_dir / "raw").glob("*/c*/run-*.json")):
        data = json.loads(path.read_text())
        if "benchmark" not in data:
            raise SystemExit(f"missing benchmark fields: {path}")
        runs.append(data)
    if not runs:
        raise SystemExit(f"no raw benchmark results under {run_dir / 'raw'}")
    return metadata, runs


def summarize(metadata: dict[str, Any], runs: list[dict[str, Any]]) -> dict[str, Any]:
    grouped: dict[tuple[str, int, str], list[dict[str, Any]]] = defaultdict(list)
    for run in runs:
        benchmark = run["benchmark"]
        key = (
            benchmark["scenario"],
            benchmark["concurrency"],
            benchmark["framework"],
        )
        grouped[key].append(run)

    groups = []
    for (scenario, concurrency, framework), samples in grouped.items():
        item = {
            "scenario": scenario,
            "concurrency": concurrency,
            "framework": framework,
        }
        item.update(aggregate(samples))
        groups.append(item)

    scenario_order = {
        name: index for index, name in enumerate(metadata["config"]["scenarios"])
    }
    framework_order = {name: index for index, name in enumerate(FRAMEWORKS)}
    groups.sort(
        key=lambda item: (
            item["concurrency"],
            scenario_order.get(item["scenario"], 10_000),
            framework_order.get(item["framework"], 10_000),
        )
    )
    return {"schemaVersion": 1, "metadata": metadata, "groups": groups}


def format_number(value: float) -> str:
    return f"{value:,.0f}"


def terminal_report(summary: dict[str, Any]) -> str:
    lines = []
    current_concurrency = None
    for item in summary["groups"]:
        if item["concurrency"] != current_concurrency:
            current_concurrency = item["concurrency"]
            lines.extend(
                [
                    "",
                    f"Concurrency {current_concurrency}",
                    f"{'scenario':<18} {'framework':<7} {'req/s median ± MAD':>24} {'p99 ms':>9} {'TTFB99':>9} {'CPU %':>9} {'RSS MiB':>9}",
                ]
            )
        rps = item["requestsPerSecond"]
        lines.append(
            f"{item['scenario']:<18} {item['framework']:<7} "
            f"{format_number(rps['median']):>11} ± {format_number(rps['mad']):<8} "
            f"{item['latencyMs']['p99Median']:>9.2f} "
            f"{item['firstByteMs']['p99Median']:>9.2f} "
            f"{item['serverCpuPercent']['median']:>9.1f} "
            f"{item['serverPeakRssMiB']['median']:>9.1f}"
        )
    return "\n".join(lines).lstrip()


def html_report(summary: dict[str, Any]) -> str:
    metadata = summary["metadata"]
    rows_by_concurrency: dict[int, list[str]] = defaultdict(list)
    for item in summary["groups"]:
        rps = item["requestsPerSecond"]
        latency = item["latencyMs"]
        first_byte = item["firstByteMs"]
        cpu = item["serverCpuPercent"]
        rss = item["serverPeakRssMiB"]
        rows_by_concurrency[item["concurrency"]].append(
            "<tr>"
            f"<td>{html.escape(item['scenario'])}</td>"
            f"<td class=\"{html.escape(item['framework'])}\">{html.escape(item['framework'])}</td>"
            f"<td>{item['samples']}</td>"
            f"<td>{format_number(rps['median'])}</td>"
            f"<td>{format_number(rps['mad'])}</td>"
            f"<td>{latency['p50Median']:.2f}</td>"
            f"<td>{latency['p95Median']:.2f}</td>"
            f"<td>{latency['p99Median']:.2f}</td>"
            f"<td>{first_byte['p50Median']:.2f}</td>"
            f"<td>{first_byte['p99Median']:.2f}</td>"
            f"<td>{cpu['median']:.1f}</td>"
            f"<td>{rss['median']:.1f}</td>"
            "</tr>"
        )

    sections = []
    for concurrency in sorted(rows_by_concurrency):
        sections.append(
            f"<h2>Concurrency {concurrency}</h2>"
            "<div class=\"table-wrap\"><table><thead><tr>"
            "<th>Scenario</th><th>Framework</th><th>n</th>"
            "<th>req/s median</th><th>req/s MAD</th>"
            "<th>p50 ms</th><th>p95 ms</th><th>p99 ms</th>"
            "<th>TTFB p50 ms</th><th>TTFB p99 ms</th>"
            "<th>server CPU %</th><th>peak RSS MiB</th>"
            "</tr></thead><tbody>"
            + "".join(rows_by_concurrency[concurrency])
            + "</tbody></table></div>"
        )

    source = metadata["source"]
    tools = metadata["tools"]
    host = metadata["host"]
    dirty = " (dirty)" if source["dirty"] else ""
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>rkt HTTP benchmark report</title>
<style>
:root {{ color-scheme: light dark; font-family: system-ui, sans-serif; }}
body {{ margin: 2rem auto; max-width: 1160px; padding: 0 1rem; }}
h1 {{ margin-bottom: .35rem; }} h2 {{ margin-top: 2rem; }}
.meta, .note {{ color: #777; line-height: 1.5; }}
.table-wrap {{ overflow-x: auto; }}
table {{ border-collapse: collapse; width: 100%; font-variant-numeric: tabular-nums; }}
th, td {{ border-bottom: 1px solid #9995; padding: .45rem .6rem; text-align: right; white-space: nowrap; }}
th:first-child, td:first-child, th:nth-child(2), td:nth-child(2) {{ text-align: left; }}
td.rkt {{ color: #d65343; font-weight: 650; }}
td.axum {{ color: #468bd8; }} td.actix {{ color: #3ca966; }}
code {{ overflow-wrap: anywhere; }}
</style>
</head>
<body>
<h1>rkt HTTP benchmark report</h1>
<div class="meta">
Run {html.escape(metadata['startedAt'])} · HTTP/1.1 · isolated sequential servers<br>
Commit <code>{html.escape(source['commit'])}</code>{dirty}<br>
oha {html.escape(tools['oha'])} · {html.escape(host['cpuModel'])} · {html.escape(host['kernel'])}<br>
CPU affinity {html.escape(host['affinity'] or 'not recorded')} · governor {html.escape(host['governor'] or 'not recorded')}
</div>
<p class="note">Central values are medians across repetitions. Dispersion is median absolute deviation (MAD). CPU and RSS cover the server process, not the load generator.</p>
{''.join(sections)}
<p class="note">The <code>query-borrowed</code> case borrows the rkt message value while Axum and Actix deserialize an owned value. Use <code>query-owned</code> for the like-for-like ownership comparison.</p>
</body>
</html>
"""


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("run_dir", type=Path)
    args = parser.parse_args()

    metadata, runs = load_run(args.run_dir)
    summary = summarize(metadata, runs)
    (args.run_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    (args.run_dir / "report.html").write_text(html_report(summary))
    print(terminal_report(summary))


if __name__ == "__main__":
    main()
