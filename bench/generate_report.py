#!/usr/bin/env python3
"""bench/generate_report.py — Benchmark dashboard / report generator.

Reads CSV output from run_all.sh and generates a Markdown report with
summary tables and comparative analysis.

Usage:
    python3 bench/generate_report.py results.csv [--output report.md]
    python3 bench/generate_report.py results.csv --json
"""

import argparse
import csv
import json
import statistics
import sys
from collections import defaultdict
from pathlib import Path


FAILURES: dict = defaultdict(lambda: defaultdict(list))


def load_csv(path: str) -> list[dict]:
    """Load benchmark results from CSV.

    Runs recorded as ERROR (process failed) or WRONG (output did not match the
    reference) are excluded from the statistics and collected in FAILURES.
    """
    rows = []
    FAILURES.clear()
    with open(path, newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            if row["time_ms"] in ("ERROR", "WRONG"):
                FAILURES[row["benchmark"]][row["language"]].append(row["time_ms"])
                continue
            row["time_ms"] = float(row["time_ms"])
            rows.append(row)
    return rows


def load_meta(csv_path: str) -> dict:
    """Read `<csv>.meta` (key: value lines) written by run_all.sh, if present."""
    meta = {}
    p = Path(csv_path + ".meta")
    if p.exists():
        for line in p.read_text().splitlines():
            if ": " in line:
                k, v = line.split(": ", 1)
                meta[k.strip()] = v.strip()
    return meta


def aggregate(rows: list[dict]) -> dict:
    """Aggregate results by benchmark and language.

    Returns: { benchmark: { language: { median, mean, min, max, runs } } }
    """
    grouped: dict[str, dict[str, list[float]]] = defaultdict(lambda: defaultdict(list))
    for row in rows:
        grouped[row["benchmark"]][row["language"]].append(row["time_ms"])

    result = {}
    for bench, langs in sorted(grouped.items()):
        result[bench] = {}
        for lang, times in sorted(langs.items()):
            result[bench][lang] = {
                "median": statistics.median(times),
                "mean": round(statistics.mean(times), 2),
                "min": min(times),
                "max": max(times),
                "stdev": round(statistics.stdev(times), 2) if len(times) > 1 else 0.0,
                "runs": len(times),
            }
    return result


def find_fastest(bench_data: dict) -> str:
    """Return language name with lowest median for a benchmark."""
    return min(bench_data, key=lambda lang: bench_data[lang]["median"])


def generate_markdown(data: dict, csv_path: str) -> str:
    """Generate a Markdown report from aggregated data."""
    lines = []
    lines.append("# Lumen Cross-Language Benchmark Report")
    lines.append("")
    lines.append(f"Source: `{csv_path}`")
    lines.append("")

    meta = load_meta(csv_path)
    if meta:
        lines.append("## Environment")
        lines.append("")
        for key in (
            "date",
            "repo_commit",
            "repo_dirty",
            "lumen",
            "cpu",
            "cores",
            "os",
            "runs",
            "gcc",
            "rustc",
            "go",
            "python",
        ):
            if key in meta:
                lines.append(f"- **{key}**: {meta[key]}")
        lines.append("")
        lines.append(
            "Lumen samples are `lumen run <file>` wall-clock (process start-up and "
            "compilation included). Every sample's output was checked against "
            "`bench/cross-language/<bench>/expected.txt`; failed or wrong runs are "
            "excluded from the tables and listed below."
        )
        lines.append("")

    # All languages across all benchmarks
    all_langs = sorted({lang for bench in data.values() for lang in bench})

    # --- Summary Table ---
    lines.append("## Summary (median time in ms)")
    lines.append("")
    header = (
        "| Benchmark |" + "|".join(f" {lang} " for lang in all_langs) + "| Fastest |"
    )
    sep = "|-----------|" + "|".join("------:" for _ in all_langs) + "|---------|"
    lines.append(header)
    lines.append(sep)

    for bench, langs in sorted(data.items()):
        fastest = find_fastest(langs)
        row = f"| {bench} |"
        for lang in all_langs:
            if lang in langs:
                val = langs[lang]["median"]
                marker = " **" if lang == fastest else " "
                end = "** " if lang == fastest else " "
                row += f"{marker}{val:.0f}{end}|"
            else:
                row += " - |"
        row += f" {fastest} |"
        lines.append(row)

    lines.append("")

    # --- Relative Performance ---
    lines.append("## Relative Performance (vs C baseline)")
    lines.append("")
    lines.append("Values show how many times slower than C (1.0x = same speed).")
    lines.append("")
    header2 = "| Benchmark |" + "|".join(f" {lang} " for lang in all_langs) + "|"
    sep2 = "|-----------|" + "|".join("------:" for _ in all_langs) + "|"
    lines.append(header2)
    lines.append(sep2)

    for bench, langs in sorted(data.items()):
        c_median = langs.get("c", {}).get("median")
        row = f"| {bench} |"
        for lang in all_langs:
            if lang in langs and c_median and c_median > 0:
                ratio = langs[lang]["median"] / c_median
                row += f" {ratio:.1f}x |"
            elif lang in langs:
                row += f" {langs[lang]['median']:.0f}ms |"
            else:
                row += " - |"
        lines.append(row)

    lines.append("")

    # --- Detailed Results ---
    lines.append("## Detailed Results")
    lines.append("")

    for bench, langs in sorted(data.items()):
        lines.append(f"### {bench}")
        lines.append("")
        lines.append(
            "| Language | Median (ms) | Mean (ms) | Min (ms) | Max (ms) | Stdev | Runs |"
        )
        lines.append(
            "|----------|----------:|--------:|-------:|-------:|------:|-----:|"
        )
        for lang in sorted(langs):
            s = langs[lang]
            lines.append(
                f"| {lang} | {s['median']:.1f} | {s['mean']:.1f} "
                f"| {s['min']:.1f} | {s['max']:.1f} | {s['stdev']:.1f} | {s['runs']} |"
            )
        lines.append("")

    # --- Failed / wrong runs ---
    if FAILURES:
        lines.append("## Failed or wrong runs (excluded above)")
        lines.append("")
        lines.append("| Benchmark | Language | ERROR | WRONG |")
        lines.append("|-----------|----------|------:|------:|")
        for bench in sorted(FAILURES):
            for lang in sorted(FAILURES[bench]):
                kinds = FAILURES[bench][lang]
                lines.append(
                    f"| {bench} | {lang} | {kinds.count('ERROR')} | {kinds.count('WRONG')} |"
                )
        lines.append("")

    # --- Lumen Analysis ---
    lines.append("## Lumen Performance Analysis")
    lines.append("")

    lumen_ranks = []
    for bench, langs in sorted(data.items()):
        if "lumen" not in langs:
            continue
        sorted_langs = sorted(langs.items(), key=lambda x: x[1]["median"])
        rank = next(i for i, (name, _) in enumerate(sorted_langs, 1) if name == "lumen")
        total = len(sorted_langs)
        lumen_median = langs["lumen"]["median"]
        fastest_name, fastest_data = sorted_langs[0]
        ratio = (
            lumen_median / fastest_data["median"] if fastest_data["median"] > 0 else 0
        )
        lumen_ranks.append((bench, rank, total, ratio, fastest_name))

    if lumen_ranks:
        lines.append("| Benchmark | Lumen Rank | vs Fastest | Fastest Language |")
        lines.append("|-----------|:----------:|:----------:|:----------------:|")
        for bench, rank, total, ratio, fastest in lumen_ranks:
            lines.append(f"| {bench} | {rank}/{total} | {ratio:.1f}x | {fastest} |")
        lines.append("")

        avg_ratio = statistics.mean(r[3] for r in lumen_ranks)
        lines.append(f"Average slowdown vs fastest: **{avg_ratio:.1f}x**")
        lines.append("")

    lines.append("---")
    lines.append("*Generated by `bench/generate_report.py`*")
    lines.append("")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description="Generate benchmark report")
    parser.add_argument("csv_file", help="Path to CSV results file")
    parser.add_argument("--output", "-o", help="Output markdown file (default: stdout)")
    parser.add_argument(
        "--json", action="store_true", help="Output JSON instead of Markdown"
    )
    args = parser.parse_args()

    rows = load_csv(args.csv_file)
    if not rows:
        print("No valid results found in CSV.", file=sys.stderr)
        sys.exit(1)

    data = aggregate(rows)

    if args.json:
        output = json.dumps(data, indent=2)
    else:
        output = generate_markdown(data, args.csv_file)

    if args.output:
        Path(args.output).write_text(output)
        print(f"Report written to {args.output}", file=sys.stderr)
    else:
        print(output)


if __name__ == "__main__":
    main()
