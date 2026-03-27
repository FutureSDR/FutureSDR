#!/usr/bin/env python3

from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import scipy.stats


SCRIPT_DIR = Path(__file__).resolve().parent
STYLE = SCRIPT_DIR.parent / "acmart.mplrc"
CSV = SCRIPT_DIR / "perf-data" / "fft_batch_compare.csv"
OUT = SCRIPT_DIR / "fft_batch_compare.pdf"

plt.style.use(str(STYLE))

d = pd.read_csv(CSV)


def conf_int(data, confidence=0.95):
    a = 1.0 * np.array(data)
    n = len(a)
    se = scipy.stats.sem(a)
    if (n < 2) or (se == 0):
        return np.nan
    return se * scipy.stats.t.ppf((1 + confidence) / 2.0, n - 1)


g = (
    d.groupby(["bin", "batch_size"], as_index=False)
    .agg(time_mean=("seconds", "mean"), time_conf_int=("seconds", conf_int))
    .sort_values(["bin", "batch_size"])
)

pivot_mean = g.pivot(index="batch_size", columns="bin", values="time_mean").sort_index()
pivot_ci = g.pivot(index="batch_size", columns="bin", values="time_conf_int").sort_index()
print(pivot_mean)

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=0.18, left=0.12, top=0.98, right=0.98)

if "fft-non-burn" in pivot_mean.columns:
    ax.errorbar(
        pivot_mean.index.to_numpy(),
        pivot_mean["fft-non-burn"].to_numpy(),
        yerr=pivot_ci["fft-non-burn"].to_numpy(),
        marker="o",
        label="Non-burn",
    )

if "fft-wgpu-hack" in pivot_mean.columns:
    ax.errorbar(
        pivot_mean.index.to_numpy(),
        pivot_mean["fft-wgpu-hack"].to_numpy(),
        yerr=pivot_ci["fft-wgpu-hack"].to_numpy(),
        marker="o",
        label="Burn (wgpu-hack)",
    )

ax.set_xlabel("Batch Size")
ax.set_ylabel("Average Runtime (in s)")
ax.set_ylim(0, 20)
ax.set_xscale("log")
ax.legend()

plt.savefig(OUT)
plt.close("all")
