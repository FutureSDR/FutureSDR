M17 Protocol Example (m17)
========================

## Introduction

This example demonstrates a complete M17 Digital Radio pipeline. It is split into two main components:

1. Transmitter (TX): Converts a standard `.wav` audio file into an M17-compatible RF baseband signal and saves it in a `.cf32` file.
2. Receiver (RX): Processes the RF signal recorded in a `.cf32` file and plays the audio.

## How it works:

The TX flowgraph consists of the following parts:
* Audio Input: Reads 8kHz mono 16-bit PCM from a `.wav` file. (The default is `rick.wav` file. It can be changed in the code.)
* Voice Encoding: Compresses audio using Codec2 (3200 bps).
* M17 Framing: Adds Link Setup Frames (LSF) with callsigns (e.g., DF1BBL).
* Pulse Shaping: Applies Root Raised Cosine (RRC) filtering.
* Modulation: Converts data to a FM complex baseband signal.
* File Sink: Saves the result to a `.cf32` file.

The RX flowgraph can be summarized as below:
* Demodulation: Extracts frequency information from the IQ signal.
* Synchronization: Handles DC offset removal and symbol timing recovery.
* M17 Decoding: Extracts audio and data from the M17 signal.
* Voice Decoding: Expands Codec2 packets back to audible sound.
* Audio Output: Resamples the 8kHz stream to 48kHz for system audio playback.

## How to run:
* To generate an M17 signal, ensure you have a compatible `.wav` (8kHz, 16-bit Mono) in the directory and run this command:

```sh
cargo run --release --bin tx
  ```

This will generate the `input.cf32` file.

* Run the receiver to process the generated file and play it:

```sh
cargo run --release --bin rx
  ```