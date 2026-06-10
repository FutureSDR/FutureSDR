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
print(d.groupby(['config', 'stages']).agg({'time': 'mean'}).unstack(level=[0]))
d = d.groupby(['config', 'stages']).agg({'time': [np.mean, np.var, conf_int]})

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=.192, left=.11, top=.99, right=.97)

if 'smol1' in d.index.droplevel('stages').unique():
    t = d.loc['smol1'].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Circ/Smol-1')

if 'smoln' in d.index.droplevel('stages').unique():
    t = d.loc['smoln'].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Circ/Smol-N')

if 'flow' in d.index.droplevel('stages').unique():
    t = d.loc['flow'].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Circ/Flow')

if 'smoln-spsc' in d.index.droplevel('stages').unique():
    t = d.loc['smoln-spsc'].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='SPSC/Smol-N')

if 'flow-spsc' in d.index.droplevel('stages').unique():
    t = d.loc['flow-spsc'].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='SPSC/Flow')

if 'local' in d.index.droplevel('stages').unique():
    t = d.loc['local'].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Local Domains')

ax.set_prop_cycle(None)

if 'smol1-slab' in d.index.droplevel('stages').unique():
    t = d.loc['smol1-slab'].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Slab/Smol-1', ls=':')

if 'smoln-slab' in d.index.droplevel('stages').unique():
    t = d.loc['smoln-slab'].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Slab/Smol-N', ls=':')

if 'flow-slab' in d.index.droplevel('stages').unique():
    t = d.loc['flow-slab'].reset_index();
    ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Slab/Flow', ls=':')

plt.setp(ax.get_yticklabels(), rotation=90, va="center")
ax.set_xlabel(r'\#\,Stages')
ax.set_ylabel('Execution Time (in s)')
ax.set_ylim(0)

handles, labels = ax.get_legend_handles_labels()
handles = [x[0] for x in handles]
ax.legend(handles, labels, handlelength=2.95, ncol=2)

plt.savefig('buffer_type.pdf')
plt.close('all')
