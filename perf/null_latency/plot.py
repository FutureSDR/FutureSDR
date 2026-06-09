#!/usr/bin/env python3

import pandas as pd
import numpy as np
import matplotlib.pyplot as plt

plt.style.use('../acmart.mplrc')

### latency vs stages
print("loading csv")
d = pd.read_csv('perf-data/results.csv')

print("filtering data")
d = d[d['max_copy'] == 512]
d = d[d['pipes'] == 6]
d = d[d['samples'] == 200000000]
d = d[['sdr', 'scheduler', 'stages', 'run', 'time', 'event', 'block', 'items']]

def gr_block_index(d):
    if d['event'] == 'rx':
        return d['block'] - d['stages'] - 2
    else:
        return d['block']

print("applying block index change for GR...", end='')
d['block'] = d.apply(gr_block_index, axis=1)
print("done")

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
    foo = rx.join(tx, lsuffix='_rx', how='outer')
    diff = foo.shape[0] - lat.shape[0]
    if diff > 6:
        print(f"{i} item lost {diff} of {lat.shape[0]}")
    lat = lat['time_rx'] - lat['time']
    assert np.all(lat > 0)

    t = pd.DataFrame(lat, columns=['latency'])
    t['sdr'] = i[0]
    t['scheduler'] = i[1]
    t['stages'] = i[2]

    r = pd.concat([r, t], axis=0)

r['latency'] = r['latency']/1e6

r.to_pickle("latency.data")

##############################################################
##############################################################
##############################################################
##############################################################
r = pd.read_pickle("latency.data")

def percentile(n):
    def percentile_(x):
        return np.percentile(x, n)
    percentile_.__name__ = 'percentile_%s' % n
    return percentile_

d = r.groupby(['sdr', 'scheduler', 'stages']).agg(
    {'latency': ['mean', 'std', percentile(5), percentile(95)]}
)

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=.192, left=.11, top=.99, right=.97)

def plot_series(key, label, offset):
    available = d.index.droplevel('stages').unique()
    if key not in available:
        print(f"skipping {label}: no data")
        return

    t = d.loc[key].reset_index()
    t[('latency', 'percentile_5')] = t[('latency', 'mean')] - t[('latency', 'percentile_5')]
    t[('latency', 'percentile_95')] = t[('latency', 'percentile_95')] - t[('latency', 'mean')]
    ax.errorbar(
        t['stages'] + offset,
        t[('latency', 'mean')],
        yerr=[t[('latency', 'percentile_5')], t[('latency', 'percentile_95')]],
        label=label,
    )

plot_series(('gr', 'legacy'), r'GNU\,Radio', 0)
plot_series(('fs', 'smoln'), 'Smol-N', -0.3)
plot_series(('fs', 'flow'), 'Flow', 0.3)

plt.setp(ax.get_yticklabels(), rotation=90, va="center")
ax.set_xlabel(r'\#\,Stages')
ax.set_ylabel('Latency (in ms)')
ax.set_ylim(bottom=0)

handles, labels = ax.get_legend_handles_labels()
if handles:
    handles = [x[0] for x in handles]
    ax.legend(handles, labels, handlelength=2.95)

plt.savefig('latency.pdf')
plt.close('all')

t = r.groupby(['sdr', 'scheduler', 'stages']).agg({'latency': 'mean'})
print(t.unstack(level=[0, 1]))
