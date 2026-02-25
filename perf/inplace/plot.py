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
t = d.groupby(['sdr', 'config', 'stages']).agg({'time': 'mean'})
print(t.unstack(level=[0,1]))

d = d.groupby(['sdr', 'config', 'stages']).agg({'time': ['mean', 'var', conf_int]})

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=.192, left=.11, top=.99, right=.97)

if ('gr', 'legacy') in d.index:
    t = d.loc[('gr')].reset_index()
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='GNU\\,Radio')

if ('fs', 'smoln') in d.index:
    t = d.loc[('fs', 'smoln')].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Smol-N')

if ('fs', 'flow') in d.index:
    t = d.loc[('fs', 'flow')].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Flow')

if ('fs', 'inplace-smol') in d.index:
    t = d.loc[('fs', 'inplace-smol')].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Inplace Smol')

if ('fs', 'inplace-flow') in d.index:
    t = d.loc[('fs', 'inplace-flow')].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Inplace Flow')

if ('fs', 'slab') in d.index:
    t = d.loc[('fs', 'slab')].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Flow+Slab')

plt.setp(ax.get_yticklabels(), rotation=90, va="center")
ax.set_xlabel('\\#\\,Stages')
ax.set_ylabel('Execution Time (in s)')
ax.set_ylim(0)

handles, labels = ax.get_legend_handles_labels()
handles = [x[0] for x in handles]
ax.legend(handles, labels, handlelength=2.95)

plt.savefig('add.pdf')
plt.close('all')
