#!/usr/bin/env python3

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

plt.style.use("../acmart.mplrc")

d = pd.read_csv("perf-data/results.csv")
g = (
    d.groupby(["payload", "sdr", "config"], as_index=False)
    .agg(
        time_mean=("time", "mean"),
        branch0=("branch0", "mean"),
        branch1=("branch1", "mean"),
        branch2=("branch2", "mean"),
        branch3=("branch3", "mean"),
        expected_branch0=("expected_branch0", "first"),
        expected_branch1=("expected_branch1", "first"),
        expected_branch2=("expected_branch2", "first"),
        expected_branch3=("expected_branch3", "first"),
    )
    .sort_values(["payload", "sdr", "config"])
)

pivot = g.pivot(index="payload", columns=["sdr", "config"], values="time_mean").sort_index()
print(pivot)
print()
print(
    g[
        [
            "payload",
            "sdr",
            "config",
            "branch0",
            "branch1",
            "branch2",
            "branch3",
            "expected_branch0",
            "expected_branch1",
            "expected_branch2",
            "expected_branch3",
        ]
    ]
)

payloads = pivot.index.to_numpy()
x = np.arange(len(payloads))
width = 0.24

fs_normal = (
    pivot[("fs", "normal")].to_numpy()
    if ("fs", "normal") in pivot.columns
    else np.zeros(len(payloads))
)
fs_opti = (
    pivot[("fs", "opti")].to_numpy()
    if ("fs", "opti") in pivot.columns
    else np.zeros(len(payloads))
)
gr_legacy = (
    pivot[("gr", "legacy")].to_numpy()
    if ("gr", "legacy") in pivot.columns
    else np.zeros(len(payloads))
)

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=0.2, left=0.12, top=0.98, right=0.98)

ax.bar(x - width, fs_normal, width, label="FutureSDR normal")
ax.bar(x, fs_opti, width, label="FutureSDR opti")
ax.bar(x + width, gr_legacy, width, label="GNU Radio")

ax.set_xticks(x)
ax.set_xticklabels([str(v) for v in payloads])
ax.set_xlabel("Payload Size (B)")
ax.set_ylabel("Execution Time (in s)")
ax.set_ylim(0)
ax.legend()

plt.savefig("lora.pdf")
plt.close("all")
