import pandas as pd
import numpy as np
import scipy.stats
import matplotlib.pyplot as plt

plt.style.use('../acmart.mplrc')

### latency vs stages
d = pd.read_csv('perf-data/results.csv')
d = d[d['max_copy'] == 512]
d = d[d['pipes'] == 6]
d = d[d['samples'] == 200000000]

d = d[['sdr', 'scheduler', 'stages', 'run', 'time', 'event', 'block', 'items']]

g = d.groupby(['sdr', 'scheduler', 'stages', 'run'])

r = pd.DataFrame({'sdr': pd.Series(dtype='str'),
                  'scheduler': pd.Series(dtype='str'),
                  'stages': pd.Series(dtype='int64'),
                  'latency': pd.Series(dtype='int64')})

for (i, x) in g:
    a = x[['time', 'event', 'block', 'items']]
    rx = a[a['event'] == 'rx'].set_index(['block', 'items'])
    tx = a[a['event'] == 'tx'].set_index(['block', 'items'])
    lat = rx.join(tx, lsuffix='_rx', how='inner')
    lat = lat['time_rx'] - lat['time']

    t = pd.DataFrame(lat[0], columns=['latency'])
    t['sdr'] = i[0]
    t['scheduler'] = i[1]
    t['stages'] = i[2]

    r = pd.concat([r, t], axis=0)

d = r.groupby(['sdr', 'scheduler', 'stages']).agg({'latency': [np.mean, np.std]})

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=.192, left=.11, top=.99, right=.97)

# t = d.loc[('gr')].reset_index()
# ax.errorbar(t['stages'], t[('time', 'mean')], yerr=t[('time', 'conf_int')], label='GNU\,Radio')

t = d.loc[('fs', 'smoln')].reset_index();
ax.errorbar(t['stages'], t[('latency', 'mean')], yerr=t[('latency', 'std')], label='Smol-N')

t = d.loc[('fs', 'flow')].reset_index();
ax.errorbar(t['stages'], t[('latency', 'mean')], yerr=t[('latency', 'std')], label='Flow')

plt.setp(ax.get_yticklabels(), rotation=90, va="center")
ax.set_xlabel('\#\,Stages')
ax.set_ylabel('Execution Time (in s)')
ax.set_ylim(0)

handles, labels = ax.get_legend_handles_labels()
handles = [x[0] for x in handles]
ax.legend(handles, labels, handlelength=2.95)

plt.savefig('latency.pdf')
plt.close('all')

