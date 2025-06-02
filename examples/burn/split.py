# split.py
import os
import pickle
import numpy as np
from sklearn.model_selection import train_test_split

# ─── 1) Load the pickled AMR‐Benchmark dict with Latin‐1 encoding ───
with open("/home/basti/Downloads/radioml2016-deepsigcom/RML2016.10a_dict.pkl", "rb") as f:
    data_dict = pickle.load(f, encoding="latin1")

# data_dict’s keys look like (“BPSK”, 0), (“BPSK”, 2), (“QPSK”, 0), … 
# and each value is an ndarray of shape (n_samples, 2, 128).

# ─── 2) Collect all modulation names (unique “mod” strings) ───
all_mods = sorted({mod for (mod, snr) in data_dict.keys()})
# Build a map from mod name → integer class index 0..(num_classes−1)
mod_to_idx = {mod: idx for idx, mod in enumerate(all_mods)}
num_classes = len(all_mods)
print(f"Found {num_classes} modulation classes: {all_mods}")

# ─── 3) Flatten everything into one big X_all and one big y_all ───
X_list = []
Y_list = []
for (mod, snr), arr in data_dict.items():
    # arr.shape == (n_samples, 2, 128)
    label_idx = mod_to_idx[mod]
    n = arr.shape[0]
    X_list.append(arr.astype(np.float32))        # keep as float32
    Y_list.append(np.ones(n, dtype=np.uint8) * label_idx)

# Stack them
X_all = np.vstack(X_list)    # shape: (N_total, 2, 128)
y_all = np.concatenate(Y_list)  # shape: (N_total,)
print("After stacking:", X_all.shape, y_all.shape)

# ─── 4) Shuffle + stratify by label ───
rng = np.random.RandomState(42)
perm = rng.permutation(len(X_all))
X_all = X_all[perm]
y_all = y_all[perm]

# ─── 5) Split off 10% for test ───
X_tmp, X_test, y_tmp, y_test = train_test_split(
    X_all, y_all, test_size=0.10, random_state=42, stratify=y_all
)

# ─── 6) From the remaining 90%, hold out 20% as validation (→ 18% overall) ───
X_train, X_val, y_train, y_val = train_test_split(
    X_tmp, y_tmp, test_size=0.20, random_state=42, stratify=y_tmp
)

print("After split:")
print("  X_train:", X_train.shape, "y_train:", y_train.shape)
print("  X_val:  ", X_val.shape,   "y_val:  ",   y_val.shape)
print("  X_test: ", X_test.shape,  "y_test: ",  y_test.shape)

# ─── 7) Save each split (no trailing “1” dim yet) ───
os.makedirs("preprocessed_npy", exist_ok=True)

np.save("preprocessed_npy/X_train.npy", X_train)  # float32, shape (N_train, 2, 128)
np.save("preprocessed_npy/y_train.npy", y_train)  # uint8, shape (N_train,)

np.save("preprocessed_npy/X_val.npy", X_val)
np.save("preprocessed_npy/y_val.npy", y_val)

np.save("preprocessed_npy/X_test.npy", X_test)
np.save("preprocessed_npy/y_test.npy", y_test)

print("Saved six files under ./preprocessed_npy/")
