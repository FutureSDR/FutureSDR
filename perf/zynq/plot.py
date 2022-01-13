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

d = pd.read_csv('perf-data/uncached.csv')
d = d.groupby(['sync', 'max_copy']).agg({'time': [np.mean, np.var, conf_int]})

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=.192, left=.11, top=.99, right=.97)

t = d.loc[(False)]
ax.errorbar(np.log2(t.index), t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Async, Unbufferd')

t = d.loc[(True)]
ax.errorbar(np.log2(t.index), t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='Sync, Unbufferd')

plt.setp(ax.get_yticklabels(), rotation=90, va="center")
ax.set_ylim(0)
ax.set_xlabel('DMA Buffer Size (in Samples, $2^x$)')
ax.set_ylabel('Execution Time (in s)')

handles, labels = ax.get_legend_handles_labels()
handles = [x[0] for x in handles]
ax.legend(handles, labels, handlelength=2.95)

plt.savefig('dma.pdf')
plt.close('all')

