#!/usr/bin/env python3

import pandas as pd
import numpy as np
import scipy.stats
import matplotlib.pyplot as plt

plt.style.use('../acmart.mplrc')

def conf_int(data, confidence=0.95):
    a = 1.0*np.array(data)
    n = len(a)
    m, se = np.mean(a), scipy.stats.sem(a)
    if (n < 2) or (se == 0):
        return np.nan
    h = se * scipy.stats.t.ppf((1+confidence)/2., n-1)
    return h

### throughput vs stages
d = pd.read_csv('perf-data/results.csv')
d = d[d['pipes'] == 4]
d = d[d['chunk'] == 128]
t = d.groupby(['buffer', 'buffer_size']).agg({'time': 'mean'})
print(t.unstack(level=[0]))

d = d.groupby(['buffer', 'buffer_size']).agg({'time': ['mean', 'var', conf_int]})

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=.192, left=.11, top=.99, right=.97)

def plot_series(key, label):
    if key not in d.index.droplevel('buffer_size').unique():
        print(f"skipping {label}: no data")
        return

    t = d.loc[key].reset_index()
    ax.errorbar(t['buffer_size'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label=label)

plot_series('circ', 'Circ')
plot_series('slab', 'Slab')
plot_series('spsc', 'SPSC')

plt.setp(ax.get_yticklabels(), rotation=90, va="center")
ax.set_xlabel('Buffer Size (in bytes)')
ax.set_ylabel('Execution Time (in s)')
ax.set_ylim(bottom=0)

handles, labels = ax.get_legend_handles_labels()
if handles:
    handles = [x[0] for x in handles]
    ax.legend(handles, labels, handlelength=2.95, ncol=2)

plt.savefig('buffer_size.pdf')
plt.close('all')
