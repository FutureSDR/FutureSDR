# FIR Filter Design Example

## Introduction

The application creates a sequence of three different tones. By applying a Finite Impulse Response (FIR) filter, the system isolates the target frequency and suppresses the other tones.

## How It Works

When you run the example, it builds a flowgraph consisting of the following blocks:
* Source: A generator produces a 2 kHz tone for 0.33 s, a 6 kHz tone for the next 0.33 s, and a 10 kHz tone for the final 0.33 s of each second.
* Resampler: It downsamples the signal by a factor of 2/3 to match the 44.1 kHz requirement of most sound cards.
* Bandpass Filter: The filter is specifically tuned to an approximately 400 Hz wide band centered at 6 kHz.
* AudioSink: This block plays the processed tones on your device.

## How to Run

Go to the example folder and run it:

```sh
cargo run --release
```

When the filter is enabled by default, you will hear a rhythmic 6 kHz beep separated by silence. This happens because the filter successfully rejects the 2 kHz and 10 kHz tones, allowing only the middle frequency to reach your speakers. If you modify the code to disable the filter, you will hear all three tones in a repeating sequence.
