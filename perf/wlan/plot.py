#!/usr/bin/env python3

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

plt.style.use("../acmart.mplrc")

d = pd.read_csv("perf-data/results.csv")
t = (
    d.groupby(["frames", "sdr"], as_index=False)
    .agg(time_mean=("time", "mean"))
    .sort_values(["frames", "sdr"])
)

pivot = t.pivot(index="frames", columns="sdr", values="time_mean").sort_index()
print(pivot)

frames = pivot.index.to_numpy()
x = np.arange(len(frames))
width = 0.34

fs_vals = pivot["fs"].to_numpy() if "fs" in pivot.columns else np.zeros(len(frames))
gr_vals = pivot["gr"].to_numpy() if "gr" in pivot.columns else np.zeros(len(frames))

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=0.2, left=0.12, top=0.98, right=0.98)

ax.bar(x - width / 2, fs_vals, width, label="FutureSDR")
ax.bar(x + width / 2, gr_vals, width, label="GNU Radio")

ax.set_xticks(x)
ax.set_xticklabels([str(v) for v in frames])
ax.set_xlabel("Frame Size")
ax.set_ylabel("Execution Time (in s)")
ax.set_ylim(0)
ax.legend()

plt.savefig("wlan.pdf")
plt.close("all")
