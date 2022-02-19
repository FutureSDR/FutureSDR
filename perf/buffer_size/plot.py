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
d = d.groupby(['buffer', 'buffer_size']).agg({'time': [np.mean, np.var, conf_int]})

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=.192, left=.11, top=.99, right=.97)

t = d.loc[('circ')].reset_index();
ax.errorbar(t['buffer_size'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Circ')

t = d.loc[('slab')].reset_index();
ax.errorbar(t['buffer_size'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Slab')

plt.setp(ax.get_yticklabels(), rotation=90, va="center")
ax.set_xlabel('Buffer Size (in bytes)')
ax.set_ylabel('Execution Time (in s)')
ax.set_ylim(0)

handles, labels = ax.get_legend_handles_labels()
handles = [x[0] for x in handles]
ax.legend(handles, labels, handlelength=2.95, ncol=2)

plt.savefig('buffer_size.pdf')
plt.close('all')

