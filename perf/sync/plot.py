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
d = d.groupby(['buffer', 'sync', 'scheduler', 'stages']).agg({'time': [np.mean, np.var, conf_int]})
print(d)

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=.192, left=.11, top=.99, right=.97)

t = d.loc[('circ', 'sync', 'smol1')].reset_index();
ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Sync/Smol-1')

t = d.loc[('circ', 'sync', 'smoln')].reset_index();
ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Sync/Smol-N')

t = d.loc[('circ', 'sync', 'flow')].reset_index();
ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Sync/Flow')

ax.set_prop_cycle(None)

t = d.loc[('circ', 'async', 'smol1')].reset_index();
ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Async/Smol-1', ls=':')

t = d.loc[('circ', 'async', 'smoln')].reset_index();
ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Async/Smol-N', ls=':')

t = d.loc[('circ', 'async', 'flow')].reset_index();
ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Async/Flow', ls=':')

plt.setp(ax.get_yticklabels(), rotation=90, va="center")
ax.set_xlabel('\#\,Stages')
ax.set_ylabel('Execution Time (in s)')
ax.set_ylim(0)

handles, labels = ax.get_legend_handles_labels()
handles = [x[0] for x in handles]
ax.legend(handles, labels, handlelength=2.95, ncol=2)

plt.savefig('sync.pdf')
plt.close('all')

