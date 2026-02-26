#!/usr/bin/env python3

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

plt.style.use("../acmart.mplrc")

d = pd.read_csv("perf-data/results.csv")
g = (
    d.groupby(["frames", "sdr", "config"], as_index=False)
    .agg(time_mean=("time", "mean"))
    .sort_values(["frames", "sdr", "config"])
)

pivot = g.pivot(index="frames", columns=["sdr", "config"], values="time_mean").sort_index()
print(pivot)

frames = pivot.index.to_numpy()
x = np.arange(len(frames))
width = 0.24

fs_normal = (
    pivot[("fs", "normal")].to_numpy()
    if ("fs", "normal") in pivot.columns
    else np.zeros(len(frames))
)
fs_opti = (
    pivot[("fs", "opti")].to_numpy()
    if ("fs", "opti") in pivot.columns
    else np.zeros(len(frames))
)
gr_legacy = (
    pivot[("gr", "legacy")].to_numpy()
    if ("gr", "legacy") in pivot.columns
    else np.zeros(len(frames))
)

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=0.2, left=0.12, top=0.98, right=0.98)

ax.bar(x - width, fs_normal, width, label="FutureSDR normal")
ax.bar(x, fs_opti, width, label="FutureSDR opti")
ax.bar(x + width, gr_legacy, width, label="GNU Radio")

ax.set_xticks(x)
ax.set_xticklabels([str(v) for v in frames])
ax.set_xlabel("Frame Size")
ax.set_ylabel("Execution Time (in s)")
ax.set_ylim(0)
ax.legend()

plt.savefig("wlan.pdf")
plt.close("all")
