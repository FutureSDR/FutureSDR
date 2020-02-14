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
d = d.groupby(['sdr', 'stages']).agg({'time': [np.mean, np.var, conf_int]})

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=.192, left=.11, top=.99, right=.97)

t = d.loc[('gr')]
ax.errorbar(t.index**2, t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='GNURadio')

t = d.loc[('nr')]
ax.errorbar(t.index**2, t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='FutureSDR')

plt.setp(ax.get_yticklabels(), rotation=90, va="center")
ax.set_xlabel('\#\,Pipes $\\times$ \#\,Stages')
ax.set_ylabel('Execution Time (in s)')

handles, labels = ax.get_legend_handles_labels()
handles = [x[0] for x in handles]
ax.legend(handles, labels, handlelength=2.95)

plt.savefig('msg.pdf')
plt.close('all')

