#!/usr/bin/env python3
"""Summarize native CSV runs without hiding trial spread or delivered work."""
import argparse
import csv
import json
import pathlib
import statistics


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("csv", type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args()
    rows = list(csv.DictReader(args.csv.open()))
    summary = []
    for mode in ("idle", "stream", "scroll", "both"):
        for framework in ("zgx", "gpui", "quickgui"):
            trials = [r for r in rows if r["mode"] == mode and r["framework"] == framework]
            if not trials or any(int(r["exit_code"]) for r in trials):
                parser.error(f"missing or failed trial: {framework}/{mode}")
            if len({r["repeat"] for r in trials}) != len(trials):
                parser.error(f"duplicate repeat: {framework}/{mode}")
            group = {"framework": framework, "mode": mode, "repeats": len(trials)}
            for field in ("cpu_percent_one_core", "mean_rss_bytes", "peak_rss_bytes", "workload_ticks", "wall_seconds"):
                values = [float(r[field]) for r in trials]
                group[field] = {"median": statistics.median(values), "min": min(values), "max": max(values)}
            summary.append(group)
    if len({g["repeats"] for g in summary}) != 1:
        parser.error("all framework/mode combinations must have equal repeat counts")
    args.output.write_text(json.dumps({"source_csv": str(args.csv), "groups": summary}, indent=2))
    print("| Mode | Framework | CPU median [range], % one core | Mean RSS median [range], MiB | Completed ticks [range] |")
    print("| --- | --- | ---: | ---: | ---: |")
    for group in summary:
        cpu = group["cpu_percent_one_core"]
        rss = {k: v / 1048576 for k, v in group["mean_rss_bytes"].items()}
        ticks = group["workload_ticks"]
        print(f"| {group['mode']} | {group['framework']} | {cpu['median']:.2f} [{cpu['min']:.2f}–{cpu['max']:.2f}] | {rss['median']:.2f} [{rss['min']:.2f}–{rss['max']:.2f}] | {ticks['min']:.0f}–{ticks['max']:.0f} |")


if __name__ == "__main__":
    main()
