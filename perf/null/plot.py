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


def plot_metric(d, metric, ylabel, filename):
    fig, ax = plt.subplots(1, 1)
    fig.subplots_adjust(bottom=.192, left=.11, top=.99, right=.97)

    if ('gr', 'legacy') in d.index.droplevel('stages').unique():
        t = d.loc[('gr')].reset_index()
        ax.errorbar(t['stages'], t[(metric, 'mean')], yerr=t[(metric, 'conf_int')], label=r'GNU\,Radio')

    # t = d.loc[('fs', 'smol1')].reset_index();
    # ax.errorbar(t['stages'], t[(metric, 'mean')], yerr=t[(metric, 'conf_int')], label='Smol-1')

    t = d.loc[('fs', 'smoln')].reset_index();
    ax.errorbar(t['stages'], t[(metric, 'mean')], yerr=t[(metric, 'conf_int')], label='Smol-N')

    t = d.loc[('fs', 'flow')].reset_index();
    ax.errorbar(t['stages'], t[(metric, 'mean')], yerr=t[(metric, 'conf_int')], label='Flow')

    t = d.loc[('fs', 'smoln-spsc')].reset_index();
    ax.errorbar(t['stages'], t[(metric, 'mean')], yerr=t[(metric, 'conf_int')], label='Smol-N SPSC')

    t = d.loc[('fs', 'flow-spsc')].reset_index();
    ax.errorbar(t['stages'], t[(metric, 'mean')], yerr=t[(metric, 'conf_int')], label='Flow SPSC')

    if ('fs', 'local') in d.index.droplevel('stages').unique():
        t = d.loc[('fs', 'local')].reset_index();
        ax.errorbar(t['stages'], t[(metric, 'mean')], yerr=t[(metric, 'conf_int')], label='Local Domains')

    plt.setp(ax.get_yticklabels(), rotation=90, va="center")
    ax.set_xlabel('\\#\\,Stages')
    ax.set_ylabel(ylabel)
    ax.set_ylim(0)

    handles, labels = ax.get_legend_handles_labels()
    handles = [x[0] for x in handles]
    ax.legend(handles, labels, handlelength=2.95)

    plt.savefig(filename)
    plt.close('all')


### execution time / throughput vs stages
raw = pd.read_csv('perf-data/results.csv')
raw = raw[(raw['max_copy'] == 128) & (raw['pipes'] == 4)]
# `stages` is the number of CopyN blocks per pipe; source/head/sink are not counted.
raw['copy_n_items'] = raw['pipes'] * raw['samples'] * raw['stages']
raw['throughput'] = raw['copy_n_items'] / raw['time']

t = raw.groupby(['sdr', 'config', 'stages']).agg({'time': 'mean'})
print(t.unstack(level=[0,1]))

throughput = raw.groupby(['sdr', 'config', 'stages']).agg({'throughput': 'mean'})
print(throughput.unstack(level=[0,1]))

stats = raw.groupby(['sdr', 'config', 'stages']).agg({
    'time': ['mean', 'var', conf_int],
    'throughput': ['mean', 'var', conf_int],
})

plot_metric(stats, 'time', 'Execution Time (in s)', 'null.pdf')
plot_metric(stats, 'throughput', 'CopyN Throughput (items/s)', 'throughput.pdf')
