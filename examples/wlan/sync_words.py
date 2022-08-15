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
