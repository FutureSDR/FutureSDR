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

d = pd.read_csv('perf-data/results.csv')
normalized = 'time_per_1m_messages'
d[normalized] = d['time'] / (d['pipes'] * d['burst_size']) * 1_000_000

t = d.groupby(['sdr', 'config', 'pipes', 'stages']).agg({normalized: 'mean'})
print(t.unstack(level=[0,1]))

d = d.groupby(['sdr', 'config', 'pipes', 'stages']).agg({normalized: ['mean', 'var', conf_int]})

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=.192, left=.19, top=.99, right=.97)

t = d.loc[('gr')].reset_index()
ax.errorbar(t['pipes'] * t['stages'], t[(normalized, 'mean')], yerr=t[(normalized, 'conf_int')], label='GNU Radio')

# t = d.loc[('fs', 'smol1')].reset_index()
# ax.errorbar(t['pipes'] * t['stages'], t[(normalized, 'mean')], yerr=t[(normalized, 'conf_int')], label='FutureSDR Smol-1')

t = d.loc[('fs', 'smoln')].reset_index()
ax.errorbar(t['pipes'] * t['stages'], t[(normalized, 'mean')], yerr=t[(normalized, 'conf_int')], label='FutureSDR Smol-N')

t = d.loc[('fs', 'flow')].reset_index()
ax.errorbar(t['pipes'] * t['stages'], t[(normalized, 'mean')], yerr=t[(normalized, 'conf_int')], label='FutureSDR Flow')

t = d.loc[('fs', 'local')].reset_index()
ax.errorbar(t['pipes'] * t['stages'], t[(normalized, 'mean')], yerr=t[(normalized, 'conf_int')], label='FutureSDR Local')

plt.setp(ax.get_yticklabels(), rotation=90, va="center")
ax.set_xlabel('\\#\\,Pipes $\\times$ \\#\\,Stages')
ax.set_ylabel('Time per 1M Messages (s)')
ax.set_yscale('log')

handles, labels = ax.get_legend_handles_labels()
handles = [x[0] for x in handles]
ax.legend(handles, labels, handlelength=2.95)

plt.savefig('msg.pdf')
plt.close('all')
