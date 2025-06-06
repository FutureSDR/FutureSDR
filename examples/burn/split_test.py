# split.py
import os
import pickle
import numpy as np
from sklearn.model_selection import train_test_split

with open(
    "/home/basti/Downloads/radioml2016-deepsigcom/RML2016.10a_dict.pkl",
    "rb",
) as f:
    data_dict = pickle.load(f, encoding="iso-8859-1")

# data_dict’s keys look like (“BPSK”, 0), (“BPSK”, 2), (“QPSK”, 0),
# and each value is an ndarray of shape (n_samples, 2, 128).

all_mods = sorted({mod for (mod, snr) in data_dict.keys()})
mod_to_idx = {mod: idx for idx, mod in enumerate(all_mods)}
num_classes = len(all_mods)
print(f"Found {num_classes} modulation classes: {all_mods}")
print(f"Mapping {mod_to_idx}")

samples = []
snrs = []
mods = []
for (mod, snr), arr in data_dict.items():
    label_idx = mod_to_idx[mod]
    n = arr.shape[0]

    # Normalization accross all samples of a config (mod and snr)
    power = np.mean(arr ** 2, axis=(1, 2))        # shape: (n_samples,)
    rms = np.sqrt(power + 1e-12)                  # shape: (n_samples,)
    arr_normalized = arr / rms[:, None, None]     # broadcast to (n_samples, 2, 128)

    samples.append(arr_normalized)
    snrs.append(np.ones(n) * snr)
    mods.append(np.ones(n) * label_idx)

# Stack them
samples = np.vstack(samples)   # → shape: (N_total, 2, 128)
snrs = np.concatenate(snrs)    # → shape: (N_total,)
mods = np.concatenate(mods)    # → shape: (N_total,)

print("After stacking:", samples.shape, snrs.shape, mods.shape)

perm = np.random.permutation(samples.shape[0])
samples = samples[perm]
snrs = snrs[perm]
mods = mods[perm]

print("After permutation:", samples.shape, snrs.shape, mods.shape)

# Save them
os.makedirs("preprocessed_npy", exist_ok=True)
np.save("preprocessed_npy/samples.npy", samples.astype(np.float32))
np.save("preprocessed_npy/snrs.npy", snrs.astype(np.float32))
np.save("preprocessed_npy/mods.npy", mods.astype(np.uint8))

print("Saved six files under ./preprocessed_npy/")

