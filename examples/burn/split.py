# split.py
import os
import pickle
import numpy as np
from sklearn.model_selection import train_test_split

# ─── 1) Load the pickled AMR‐Benchmark dict with Latin‐1 encoding ───
with open(
    "/home/basti/Downloads/radioml2016-deepsigcom/RML2016.10a_dict.pkl",
    "rb",
) as f:
    data_dict = pickle.load(f, encoding="iso-8859-1")

# data_dict’s keys look like (“BPSK”, 0), (“BPSK”, 2), (“QPSK”, 0), … 
# and each value is an ndarray of shape (n_samples, 2, 128).

# ─── 2) Collect all modulation names (unique “mod” strings) ───
all_mods = sorted({mod for (mod, snr) in data_dict.keys()})
mod_to_idx = {mod: idx for idx, mod in enumerate(all_mods)}
num_classes = len(all_mods)
print(f"Found {num_classes} modulation classes: {all_mods}")
print(f"Mapping {mod_to_idx}")

# ─── 3) Flatten everything into one big X_all and one big y_all ───
X_list = []
Y_list = []
for (mod, snr), arr in data_dict.items():
    # arr.shape == (n_samples, 2, 128)
    label_idx = mod_to_idx[mod]
    n = arr.shape[0]

    # ─── Option 1: Per‐sample unit‐power normalization ───
    arr = arr.astype(np.float32)
    power = np.mean(arr ** 2, axis=(1, 2))          # shape: (n_samples,)
    rms = np.sqrt(power + 1e-12)                   # shape: (n_samples,)
    arr_normalized = arr / rms[:, None, None]      # broadcast to (n_samples, 2, 128)

    X_list.append(arr_normalized)                            
    Y_list.append(np.ones(n, dtype=np.uint8) * label_idx)

# Stack them
X_all = np.vstack(X_list)      # → shape: (N_total, 2, 128)
y_all = np.concatenate(Y_list) # → shape: (N_total,)
print("After stacking:", X_all.shape, y_all.shape)

# ─── 4) Split off 10% for test ───
X_tmp, X_test, y_tmp, y_test = train_test_split(
    X_all, y_all, test_size=0.10, random_state=42, stratify=y_all
)

# ─── 5) From remaining 90%, hold out 20% as validation (→ 18% overall) ───
X_train, X_val, y_train, y_val = train_test_split(
    X_tmp, y_tmp, test_size=0.20, random_state=42, stratify=y_tmp
)

print("After split:")
print("  X_train:", X_train.shape, "y_train:", y_train.shape)
print("  X_val:  ", X_val.shape,   "y_val:  ",   y_val.shape)
print("  X_test: ", X_test.shape,  "y_test: ",  y_test.shape)

# ─── 6) Save each split ───
os.makedirs("preprocessed_npy", exist_ok=True)
np.save("preprocessed_npy/X_train.npy", X_train)  
np.save("preprocessed_npy/Y_train.npy", y_train)  
np.save("preprocessed_npy/X_val.npy",   X_val)    
np.save("preprocessed_npy/Y_val.npy",   y_val)    
np.save("preprocessed_npy/X_test.npy",  X_test)   
np.save("preprocessed_npy/Y_test.npy",  y_test)   

print("Saved six files under ./preprocessed_npy/")
