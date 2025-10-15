import pandas as pd
import numpy as np
import scipy.stats
import matplotlib.pyplot as plt
import seaborn as sns

plt.style.use('../../perf/acmart.mplrc')

def conf_int(data, confidence=0.95):
    a = 1.0*np.array(data)
    n = len(a)
    m, se = np.mean(a), scipy.stats.sem(a)
    if (n < 2) or (se == 0):
        return np.nan
    h = se * scipy.stats.t.ppf((1+confidence)/2., n-1)
    return h

modulations = {'8PSK': 0, 'AM-DSB': 1, 'AM-SSB': 2, 'BPSK': 3, 'CPFSK': 4, 'GFSK': 5, 'PAM4': 6, 'QAM16': 7, 'QAM64': 8, 'QPSK': 9, 'WBFM': 10}
modulation_names = {v: k for k, v in modulations.items()}

### throughput vs stages
d = pd.read_csv('foo.csv')
d.columns = d.columns.str.strip()
d['correct'] = d['mod'] == d['pred']
d['mod'] = d['mod'].replace(modulation_names)
d['pred'] = d['pred'].replace(modulation_names)

dg = d.groupby(['snr', 'mod'])['correct'].mean().reset_index()
pivot = dg.pivot(index='snr', columns='mod', values='correct')
print(pivot)

# Plot the data
pivot.plot(marker='o')
plt.title('Correctness vs. SNR by Modulation Scheme')
plt.xlabel('SNR')
plt.ylabel('Correctness')
plt.grid(True)
plt.ylim(0, 1)
plt.legend(title='Modulation')
plt.tight_layout()
plt.show()

# Group by 'snr' and calculate the mean correctness
dg = d.groupby('snr')['correct'].mean().reset_index()
print(dg)

plt.figure(figsize=(8, 6))
plt.plot(dg['snr'], dg['correct'], marker='o', label='Overall Correctness')
plt.title('Overall Correctness vs. SNR')
plt.xlabel('SNR')
plt.ylabel('Correctness')
plt.grid(True)
plt.ylim(0, 1)
plt.tight_layout()
plt.show()

df_filtered = d[d["snr"] == 18]
conf_matrix = pd.crosstab(df_filtered["mod"], df_filtered["pred"])
plt.figure(figsize=(10, 8))
sns.heatmap(conf_matrix, annot=True, fmt="d", cmap="Blues")
plt.title("Confusion Matrix at SNR = 18")
plt.xlabel("Predicted Modulation")
plt.ylabel("True Modulation")
plt.tight_layout()
plt.show()

