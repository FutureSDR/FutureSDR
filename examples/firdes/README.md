FIR Filter Design Example (firdes)
========================

## Introduction
The application creates a sequence of three different tones. By applying a Finite Impulse Response (FIR) filter, the system isolates the target frequency and suppresses the remaining tones.

## How it works:
When you run the example, it will build a flowgraph consisting of the following blocks:
* Source: A generator produces a 2 kHz tone for 0.33s, a 6 kHz tone for the next 0.33s, and a 10 kHz tone for the final 0.33s of each second.
* Resampler: It downsamples the signal by a factor of 2/3 to match the 44.1 kHz requirement of most sound cards.
* Bandpass Filter: The filter is specifically tuned to an approximately 400 Hz wide band centered at 6 kHz.
* AudioSink: This block plays the processed tones on your device.

When the filter is enabled (default), you will hear a rhythmic 6 kHz beeping between silence instances. This happens because the filter successfully rejects the 2 kHz and 10 kHz tones, allowing only the middle frequency to reach your speakers. If you modify the code to disable the filter, you will hear all three tones in a repeating sequence.

## How to run:
- go to the example folder and run it:
  ```bash
  $ cd FutureSDR/examples/firdes

  $ cargo run --release
  ```


