#!/usr/bin/env python3
"""Render a waveform with marker overlays.

Reads two CSVs: a waveform (``time_s,value``) and a markers list
(``position_seconds``). Plots the waveform as a thin line with vertical
lines at each marker position.

Usage:
    plot_markers.py WAVEFORM.csv MARKERS.csv OUTPUT.svg [--title TITLE]
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
    parser.add_argument("waveform", help="path to waveform CSV (time_s, value)")
    parser.add_argument("markers", help="path to markers CSV (position_seconds)")
    parser.add_argument("output", help="path to output SVG")
    parser.add_argument("--title", default="", help="optional plot title")
    args = parser.parse_args()

    times: list[float] = []
    values: list[float] = []
    with open(args.waveform, newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            times.append(float(row["time_s"]))
            values.append(float(row["value"]))

    markers: list[float] = []
    with open(args.markers, newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            markers.append(float(row["position_seconds"]))

    fig, ax = plt.subplots(figsize=(10, 4))
    ax.plot(times, values, color="tab:blue", lw=0.5, label="Audio")
    for index, marker in enumerate(markers):
        label = "Detected marker" if index == 0 else None
        ax.axvline(marker, color="tab:red", lw=1.0, alpha=0.7, label=label)
    ax.set_xlabel("Time (s)")
    ax.set_ylabel("Amplitude")
    if args.title:
        ax.set_title(args.title)
    ax.grid(True, alpha=0.3)
    if markers:
        ax.legend(loc="upper right", fontsize=9)
    fig.tight_layout()
    fig.savefig(args.output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
