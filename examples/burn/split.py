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

# "Adapted from the code (https://github.com/leena201818/radiom) contributed by leena201818"
# import pickle
# import numpy as np
# def l2_normalize(x, axis=-1):
#     y = np.max(np.sum(x ** 2, axis, keepdims=True), axis, keepdims=True)
#     return x / np.sqrt(y)
#    
# # def load_data(filename=r'/home/xujialang/ZhangFuXin/AMR/tranining/RML2016.10a_dict.pkl'):
# def load_data(
#             filename=r'E:\Richard_zhangxx\My_Research\AMR\Thesis_code\Thesis_code1080ti-experiment\data\RML2016.10a_dict.pkl'):
#     Xd =pickle.load(open(filename,'rb'),encoding='iso-8859-1')#Xd2(22W,2,128)
#     mods,snrs = [sorted(list(set([k[j] for k in Xd.keys()]))) for j in [0,1] ]
#     X = []
#     lbl = []
#     train_idx=[]
#     val_idx=[]
#     np.random.seed(2016)
#     a=0
#
#     for mod in mods:
#         for snr in snrs:
#             X.append(Xd[(mod,snr)])     #ndarray(1000,2,128)
#             for i in range(Xd[(mod,snr)].shape[0]):
#                 lbl.append((mod,snr))
#             train_idx+=list(np.random.choice(range(a*1000,(a+1)*1000), size=600, replace=False))
#             val_idx+=list(np.random.choice(list(set(range(a*1000,(a+1)*1000))-set(train_idx)), size=200, replace=False))
#             a+=1
#     X = np.vstack(X)                    #(220000,2,128)  mods * snr * 1000,total 220000 samples  chui zhi fang xiang dui die
#     n_examples=X.shape[0]
#     test_idx = list(set(range(0,n_examples))-set(train_idx)-set(val_idx))
#     np.random.shuffle(train_idx)
#     np.random.shuffle(val_idx)
#     np.random.shuffle(test_idx)
#     X_train = X[train_idx]
#     X_val=X[val_idx]
#     X_test =  X[test_idx]
#     def to_onehot(yy):
#         yy1 = np.zeros([len(yy), len(mods)])
#         yy1[np.arange(len(yy)), yy] = 1
#         yy2=yy1
#         return yy1
#     Y_train = to_onehot(list(map(lambda x: mods.index(lbl[x][0]), train_idx)))
#     Y_val=to_onehot(list(map(lambda x: mods.index(lbl[x][0]), val_idx)))
#     Y_test = to_onehot(list(map(lambda x: mods.index(lbl[x][0]),test_idx)))
#
#     return (mods,snrs,lbl),(X_train,Y_train),(X_val,Y_val),(X_test,Y_test),(train_idx,val_idx,test_idx)
# # 下的代码只有在第一种情况下（即文件作为脚本直接执行）才会被执行，而 import 到其他脚本中是不会被执行的
# if __name__ == '__main__':
#     (mods, snrs, lbl), (X_train, Y_train),(X_val,Y_val), (X_test, Y_test), (train_idx,val_idx,test_idx) = load_data()
