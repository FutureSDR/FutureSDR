Key Fob Decoder Example (keyfob)
========================

## Introduction

This example demonstrates how to decode radio signals from a common key fob using FutureSDR. The application captures the signal, measures the duration of each pulse, and identifies whether it represents a command like Open, Close, or Trunk.

## How it works:
The flowgraph consists of the following blocks:
* Source: Captures raw IQ samples from an SDR or a recorded file at 4 MHz.
* Resampler: Downsamples the signal to 250 kHz to match the decoder's timing requirements.
* Demodulator: Converts the complex signal into "magnitude squared" values to detect the presence of energy (OOK).
* Moving Average: Removes the DC offset and centers the signal.
* Low Pass Filter: Smooths out the noise.
* Slicer: Converts analog signal into a stream of digital 0s and 1s.
* Decoder: Analyze the timing of the pulses. It looks for a specific start sequence (preamble) and then interprets the following bits as a command.


## How to run:
* Plug in your SDR.
* go to the example folder and run it: 

 ```bash
  $ cd FutureSDR/examples/keyfob

  $ cargo run --release 
  ``````

* If you have a pre-recorded key fob signal in a `cf32` file, run it:

```sh
cargo run --release -- --file {file_name}.cf32 
  ```

* When a valid signal is detected and decoded, you will see a bit string and the action type in your terminal. 
* The string of bits represents the unique ID of the keyfob followed by the specific button code. The decoder identifies the action based on the trailing bit sequence.
