#!/usr/bin/env python3
"""Render a frequency response from CSV.

CSV must have ``freq_hz`` as the first column. Remaining columns are
plotted as overlaid curves on a log-frequency axis. Column headers are
used as the legend labels.

Usage:
    plot_freqz.py INPUT.csv OUTPUT.svg [--title TITLE] [--ylabel YLABEL]
"""

from __future__ import annotations

import argparse
import csv
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", help="path to input CSV")
    parser.add_argument("output", help="path to output SVG")
    parser.add_argument("--title", default="", help="optional plot title")
    parser.add_argument(
        "--ylabel",
        default="Magnitude (dB)",
        help="y-axis label (default: Magnitude (dB))",
    )
    args = parser.parse_args()

    freqs: list[float] = []
    curves: dict[str, list[float]] = {}
    with open(args.input, newline="") as f:
        reader = csv.DictReader(f)
        if not reader.fieldnames:
            print(f"empty CSV: {args.input}", file=sys.stderr)
            return 2
        freq_col = reader.fieldnames[0]
        value_cols = reader.fieldnames[1:]
        for col in value_cols:
            curves[col] = []
        for row in reader:
            freqs.append(float(row[freq_col]))
            for col in value_cols:
                curves[col].append(float(row[col]))

    if not curves:
        print(f"no value columns in {args.input}", file=sys.stderr)
        return 2

    fig, ax = plt.subplots(figsize=(8, 4.5))
    for name, values in curves.items():
        ax.semilogx(freqs, values, label=name)
    ax.set_xlabel("Frequency (Hz)")
    ax.set_ylabel(args.ylabel)
    ax.grid(True, which="both", alpha=0.3)
    if args.title:
        ax.set_title(args.title)
    if len(curves) > 1:
        ax.legend(fontsize=9)
    fig.tight_layout()
    fig.savefig(args.output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
