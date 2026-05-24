#!/usr/bin/env python3
"""Render a pole-zero plot from a coefficient CSV.

CSV must have ``b0, b1, b2, a1, a2`` columns (a0 is assumed 1 since
coefficients are normalized). Optional ``name`` column provides labels
when multiple filter rows are present.

Usage:
    plot_pz.py INPUT.csv OUTPUT.svg [--title TITLE]
"""

from __future__ import annotations

import argparse
import csv
import sys

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from scipy.signal import tf2zpk


COLORS = ["tab:blue", "tab:orange", "tab:green", "tab:red", "tab:purple"]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", help="path to input CSV")
    parser.add_argument("output", help="path to output SVG")
    parser.add_argument("--title", default="", help="optional plot title")
    args = parser.parse_args()

    filters: list[tuple[str, np.ndarray, np.ndarray]] = []
    with open(args.input, newline="") as f:
        reader = csv.DictReader(f)
        for index, row in enumerate(reader):
            name = row.get("name", f"filter_{index}")
            b = [float(row["b0"]), float(row["b1"]), float(row["b2"])]
            a = [1.0, float(row["a1"]), float(row["a2"])]
            z, p, _ = tf2zpk(b, a)
            filters.append((name, z, p))

    if not filters:
        print(f"no rows in {args.input}", file=sys.stderr)
        return 2

    fig, ax = plt.subplots(figsize=(5.5, 5.5))
    theta = np.linspace(0, 2 * np.pi, 256)
    ax.plot(np.cos(theta), np.sin(theta), "k--", lw=0.5, alpha=0.5)
    ax.axhline(0, color="k", lw=0.3, alpha=0.5)
    ax.axvline(0, color="k", lw=0.3, alpha=0.5)
    ax.set_aspect("equal", adjustable="box")
    ax.set_xlim(-1.3, 1.3)
    ax.set_ylim(-1.3, 1.3)

    for index, (name, zeros, poles) in enumerate(filters):
        color = COLORS[index % len(COLORS)]
        if len(zeros) > 0:
            ax.scatter(
                zeros.real,
                zeros.imag,
                marker="o",
                facecolors="none",
                edgecolors=color,
                s=80,
                label=f"{name} zeros",
            )
        if len(poles) > 0:
            ax.scatter(
                poles.real,
                poles.imag,
                marker="x",
                color=color,
                s=80,
                label=f"{name} poles",
            )

    ax.set_xlabel("Re")
    ax.set_ylabel("Im")
    if args.title:
        ax.set_title(args.title)
    ax.legend(fontsize=8, loc="upper right")
    ax.grid(True, alpha=0.2)
    fig.tight_layout()
    fig.savefig(args.output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
