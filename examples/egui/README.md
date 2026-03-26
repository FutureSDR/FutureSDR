# Egui Spectrum View

## Introduction

This example demonstrates how to use [egui](https://www.egui.rs/) with FutureSDR. It renders a custom GL-based widget that shows a line plot of the spectrum.

## How It Works

It demonstrates two possibilities for integrating egui.

- The `combined` binary runs the FutureSDR flowgraph and the egui GUI in one process.
- The `egui` and `fg` binaries split the GUI from the DSP. The two components are connected via a WebSocket.

The DSP flowgraph consists of the following key blocks:

* Seify Source: Interfaces with the SDR hardware to stream raw IQ samples.
* FFT Block: Converts the time-domain signal to the frequency domain with a size of 2048.
* Magnitude Square: Calculates the squared magnitude to determine the power.
* Moving Average: Smooths the spectrum by averaging consecutive frames.
* Channel / Websocket Sink: Passes the processed data to the UI either via local memory channels or WebSockets.


## How to Run

For the split configuration, run the backend:

```sh
cargo run --release --bin fg
```

and then the GUI:

```sh
cargo run --release --bin egui
```


For the combined configuration, run:

```sh
cargo run --release --bin combined
```
