#!/usr/bin/env python3
"""Render a time-domain plot (impulse, step, envelope) from CSV.

CSV must have a time column (``time_s`` or ``sample``) as the first
column. Remaining columns are plotted as overlaid curves with column
headers as legend labels.

Usage:
    plot_time.py INPUT.csv OUTPUT.svg [--title TITLE] [--ylog] [--ylabel YLABEL]
"""

from __future__ import annotations

import argparse
import csv
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", help="path to input CSV")
    parser.add_argument("output", help="path to output SVG")
    parser.add_argument("--title", default="", help="optional plot title")
    parser.add_argument(
        "--ylog",
        action="store_true",
        help="render |amplitude| on a log y-axis (useful for decay tails)",
    )
    parser.add_argument(
        "--ylabel",
        default="",
        help="y-axis label (default depends on --ylog)",
    )
    args = parser.parse_args()

    times: list[float] = []
    curves: dict[str, list[float]] = {}
    with open(args.input, newline="") as f:
        reader = csv.DictReader(f)
        if not reader.fieldnames:
            print(f"empty CSV: {args.input}", file=sys.stderr)
            return 2
        time_col = reader.fieldnames[0]
        value_cols = reader.fieldnames[1:]
        for col in value_cols:
            curves[col] = []
        for row in reader:
            times.append(float(row[time_col]))
            for col in value_cols:
                curves[col].append(float(row[col]))

    if not curves:
        print(f"no value columns in {args.input}", file=sys.stderr)
        return 2

    xlabel = "Time (s)" if time_col == "time_s" else time_col

    fig, ax = plt.subplots(figsize=(8, 4))
    for name, values in curves.items():
        if args.ylog:
            ax.semilogy(times, np.abs(values) + 1e-12, label=name)
        else:
            ax.plot(times, values, label=name)
    ax.set_xlabel(xlabel)
    if args.ylabel:
        ax.set_ylabel(args.ylabel)
    else:
        ax.set_ylabel("|Amplitude|" if args.ylog else "Amplitude")
    ax.grid(True, alpha=0.3)
    if args.title:
        ax.set_title(args.title)
    if len(curves) > 1:
        ax.legend(fontsize=9)
    fig.tight_layout()
    fig.savefig(args.output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
