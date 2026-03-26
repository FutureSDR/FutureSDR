# Key Fob Decoder Example

## Introduction

This example demonstrates how to decode radio signals from a common car key fob using FutureSDR. The application captures the signal, performs Manchester decoding, and parses the frame to output the identifier, rolling code, and command (`open`, `close`, or `trunk`).

## How It Works

The flowgraph consists of the following blocks:
* Source: Captures raw IQ samples from an SDR or a recorded file at 4 MHz.
* Resampler: Downsamples the signal to 250 kHz to match the decoder's timing requirements.
* Demodulator: Converts the complex signal into "magnitude squared" values to detect the presence of energy (OOK).
* Moving Average: Removes the DC offset and centers the signal.
* Low Pass Filter: Smooths out the noise.
* Slicer: Converts the analog signal into a stream of digital 0s and 1s.
* Decoder: Analyzes the timing of the pulses. It looks for a specific start sequence (preamble) and then interprets the following bits as a command.

## How to Run

Plug in your SDR and run the decoder with the following command:

```sh
cargo run --release
```

If you have a pre-recorded key fob signal in a `cf32` file, you can decode it with:

```sh
cargo run --release -- --file {file_name}.cf32
```

When a valid signal is detected and successfully decoded, the decoder outputs the bit string and the command in your terminal. The bit string consists of a type ID, followed by the rolling code, followed by the action (`open`, `close`, or `trunk`).
