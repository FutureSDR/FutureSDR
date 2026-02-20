FutureSDR + Egui Spectrum Analyzer (egui)
=======================

## Introduction
This project provides a real-time spectrum analyzer for SDR devices using the FutureSDR framework and the egui library for the graphical interface. It utilizes hardware-accelerated rendering to visualize signals processed by the SDR flowgraph.

## How it works:
The application architecture is divided into a signal processing backend and a graphical frontend. The DSP flowgraph consists of the following key blocks:

* Seify Source: Interfaces with the SDR hardware to stream raw IQ samples.
* FFT Block: Converts the time-domain signal to the frequency domain with a size of 2048.
* Magnitude Square: Calculates the squared magnitude to determine the power.
* Moving Average: Smooths the spectrum by averaging consecutive frames.
* Channel / Websocket Sink: Passes the processed data to the UI either via local memory channels or WebSockets.

The application features a native desktop GUI that provides real-time interaction with the RF environment:

* Real-Time Spectrum: Visualizes the frequency domain data with a color gradient.
* Frequency Slider: Updates the center frequency of the SDR device.
* Min/Max dB Sliders: Adjusts the vertical scale (power level) of the plot.

## How to run:
First, run the backend using this command:

```sh
cargo run --release --bin fg
  ```

Then, visualize the spectrum:

```sh
cargo run --release --bin egui
  ```
Optionally, you can run the complete application using the command below: 

  ```sh
cargo run --release --bin combined
  ```