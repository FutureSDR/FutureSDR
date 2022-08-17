#!/usr/bin/env python3

import numpy as np
import numpy.fft as fft
import matplotlib.pyplot as plt

short_neg  = tuple([(52**.5/24**.5) * x for x in [0, 0, 0, 0, 0, 0, 0, 0, 1+1j, 0, 0, 0, -1-1j, 0, 0, 0, 1+1j, 0, 0, 0, -1-1j, 0, 0, 0, -1-1j, 0, 0, 0, 1+1j, 0, 0, 0, 0, 0, 0, 0, -1-1j, 0, 0, 0, -1-1j, 0, 0, 0, 1+1j, 0, 0, 0, 1+1j, 0, 0, 0, 1+1j, 0, 0, 0, 1+1j, 0, 0, 0, 0, 0, 0, 0]])
long_neg = tuple([0, 0, 0, 0, 0, 0, 1, 1, -1, -1, 1, 1, -1, 1, -1, 1, 1, 1, 1, 1, 1, -1, -1, 1, 1, -1, 1, -1, 1, 1, 1, 1, 0, 1, -1, -1, 1, 1, -1, 1, -1, 1, -1, -1, -1, -1, -1, 1, 1, -1, -1, 1, -1, 1, -1, 1, 1, 1, 1, 0, 0, 0, 0, 0])

short = [0] * 64
long = [0] * 64

for i in range(64):
    short[(i + 32) % 64] = short_neg[i]
    long[(i + 32) % 64] = long_neg[i]

assert(len(short) == 64)
assert(len(long) == 64)

print("power short: " + str(sum([abs(x)**2 for x in short])))
print("power long: " + str(sum([abs(x)**2 for x in long])))

short_time = fft.ifft(short)
long_time = fft.ifft(long)

for i in range(64):
    short_time[i] *= 64 / 52**.5
    long_time[i] *= 64 / 52**.5

print("power short time: " + str(sum([abs(x)**2 for x in short_time])))
print("power long time: " + str(sum([abs(x)**2 for x in long_time])))

sync = np.concatenate((short_time, short_time, short_time[0:32], long_time[32:64], long_time, long_time))
sync[160] = 0.5 * (sync[160] + short_time[0])
print("avg sync power: " + str(sum([abs(x)**2 for x in sync])/320))

for i in range(320):
    print("Complex32::new({}, {}),".format(sync[i].real, sync[i].imag))

ref = np.fromfile("data/sync.cf32", dtype=np.complex64)

plt.ion()
plt.plot(sync.real)
plt.plot(ref.real)
plt.figure()
plt.plot(sync.imag)
plt.plot(ref.imag)



alloc_gr = np.fromfile("/home/basti/tmp/carrier-alloc-gr.cf32", dtype=np.complex64)
alloc_fs = np.fromfile("/home/basti/tmp/carrier-alloc-fs.cf32", dtype=np.complex64)

sym = 5
plt.plot(alloc_gr.real[sym*64:(sym+1)*64])
plt.plot(alloc_fs.real[sym*64:(sym+1)*64])
plt.figure()
plt.plot(alloc_gr.imag[sym*64:(sym+1)*64])
plt.plot(alloc_fs.imag[sym*64:(sym+1)*64])


fft_gr = np.fromfile("/home/basti/tmp/fft.cf32", dtype=np.complex64)
print("avg power gr: " + str(sum([abs(x)**2 for x in fft_gr])/64))
fft_fs = np.fromfile("/home/basti/tmp/fft-fs.cf32", dtype=np.complex64)
print("avg power fs: " + str(sum([abs(x)**2 for x in fft_fs])/64))

plt.ion()
plt.plot(fft_gr.real)
plt.plot(fft_fs.real)
plt.figure()
plt.plot(fft_gr.imag)
plt.plot(fft_fs.imag)

frame_gr = np.fromfile("/home/basti/tmp/frame-gr.cf32", dtype=np.complex64)
print("avg power gr: " + str(sum([abs(x)**2 for x in frame_gr])/len(frame_gr)))
frame_fs = np.fromfile("/home/basti/tmp/frame-fs.cf32", dtype=np.complex64)
print("avg power fs: " + str(sum([abs(x)**2 for x in frame_fs])/len(frame_fs)))

plt.ion()
plt.plot(frame_gr.real)
plt.plot(frame_fs.real)
plt.figure()
plt.plot(frame_gr.imag)
plt.plot(frame_fs.imag)
