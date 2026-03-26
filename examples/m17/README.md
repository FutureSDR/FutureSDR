# M17 Protocol Example

## Introduction

This example contains an incomplete M17 Digital Radio pipeline. It is split into two main components:

1. Transmitter (TX): Converts a standard `.wav` audio file into an M17-compatible baseband signal and saves it in a `.cf32` file.
2. Receiver (RX): Processes the RF signal recorded in a `.cf32` file and plays the audio.

## How It Works

The TX flowgraph consists of the following parts:
* Audio Input: Reads 8 kHz mono 16-bit PCM from a `.wav` file. (The default input file is `rick.wav`. It can be changed in the code.)
* Voice Encoding: Compresses audio using Codec2 (3200 bps).
* M17 Framing: Adds Link Setup Frames (LSF) with callsigns (e.g., DF1BBL).
* Pulse Shaping: Applies Root Raised Cosine (RRC) filtering.
* Modulation: Converts data to an FM complex baseband signal.
* File Sink: Saves the result to a `.cf32` file.

The RX flowgraph can be summarized as follows:
* Demodulation: Extracts frequency information from the IQ signal.
* Synchronization: Handles DC offset removal and symbol timing recovery.
* M17 Decoding: Extracts audio and data from the M17 signal.
* Voice Decoding: Expands Codec2 packets back to audible sound.
* Audio Output: Resamples the 8 kHz stream to 48 kHz for system audio playback.

## How to Run

To generate an M17 signal, ensure you have a compatible `.wav` file (8 kHz, 16-bit mono) in the directory and run this command:

```sh
cargo run --release --bin tx
```

This will generate the `input.cf32` file.

Run the receiver to process the generated file and play the decoded audio:

```sh
cargo run --release --bin rx
```
