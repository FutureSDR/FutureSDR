import pandas as pd
import numpy as np
import scipy.stats
import matplotlib.pyplot as plt

plt.style.use('../acmart.mplrc')

### latency vs stages
print("loading csv")
d = pd.read_csv('perf-data/results.csv')

print("filtering data")
d = d[d['max_copy'] == 512]
d = d[d['pipes'] == 6]
d = d[d['samples'] == 20000000]
d = d[['sdr', 'scheduler', 'stages', 'run', 'time', 'event', 'block', 'items']]

def gr_block_index(d):
    if d['sdr'] == 'gr' and d['event'] == 'rx':
        return d['block'] - d['stages'] * 2 - 2
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

d = r.groupby(['sdr', 'scheduler', 'stages']).agg({'latency': [np.mean, np.std, percentile(5), percentile(95)]})

fig, ax = plt.subplots(1, 1)
fig.subplots_adjust(bottom=.192, left=.11, top=.99, right=.97)

t = d.loc[('gr')].reset_index()
t[('latency', 'percentile_5')] = t[('latency', 'mean')] - t[('latency', 'percentile_5')]
t[('latency', 'percentile_95')] = t[('latency', 'percentile_95')] - t[('latency', 'mean')]
ax.errorbar(t['stages'], t[('latency', 'mean')], yerr=[t[('latency', 'percentile_5')], t[('latency', 'percentile_95')]], label='GNU\,Radio')

t = d.loc[('fs', 'smoln')].reset_index();
t[('latency', 'percentile_5')] = t[('latency', 'mean')] - t[('latency', 'percentile_5')]
t[('latency', 'percentile_95')] = t[('latency', 'percentile_95')] - t[('latency', 'mean')]
ax.errorbar(t['stages']-0.3, t[('latency', 'mean')], yerr=[t[('latency', 'percentile_5')], t[('latency', 'percentile_95')]], label='Smol-N')

t = d.loc[('fs', 'flow')].reset_index();
t[('latency', 'percentile_5')] = t[('latency', 'mean')] - t[('latency', 'percentile_5')]
t[('latency', 'percentile_95')] = t[('latency', 'percentile_95')] - t[('latency', 'mean')]
ax.errorbar(t['stages']+0.3, t[('latency', 'mean')], yerr=[t[('latency', 'percentile_5')], t[('latency', 'percentile_95')]], label='Flow')

plt.setp(ax.get_yticklabels(), rotation=90, va="center")
ax.set_xlabel('\#\,Stages')
ax.set_ylabel('Latency (in ms)')
# ax.set_ylim(0, 99)

handles, labels = ax.get_legend_handles_labels()
handles = [x[0] for x in handles]
ax.legend(handles, labels, handlelength=2.95)

plt.savefig('fir_rand_latency.pdf')
plt.close('all')

