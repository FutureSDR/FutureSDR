Spectrum Analyzer Example (spectrum)
=======================

## Introduction
This project provides a real-time spectrum analyzer using the FutureSDR framework. It is designed to visualize signals processed through a DSP flowgraph, offering the option to run computations on a standard CPU or a Vulkan-accelerated GPU. The results are displayed via a web-based GUI in real-time.

## How it works:
The application architecture consists of a signal processing backend and a WASM-based frontend. The DSP flowgraph consists of the following blocks:

* Seify Source: Interfaces with the SDR hardware to stream raw IQ samples.
* FFT Block: Converts the time-domain signal to the frequency domain with a size of 2048.
* Magnitude Square: Calculates the squared magnitude to determine the power.
* Moving Average: Smooths the spectrum by averaging consecutive frames.
* Websocket Sink: Streams the final processed frames to the Web GUI.

## Requirements

Web GUI Requirements:
* WASM Target: Should be added:
```sh
rustup target add wasm32-unknown-unknown
  ```

* Trunk: Builds and hosts the web-based interface.  

```sh
cargo install --locked trunk
  ```

Vulkan Requirements:
* Vulkan SDK: Installed on your system.
* Vulkan Compatible Drivers: Ensure your GPU supports Vulkan.

## How to run:
To start the backend flowgraph, open a terminal and run the backend in your preferred mode:

* CPU Mode:
```sh
cargo run --release --bin cpu
  ```

* Vulkan Mode:

```sh
cargo run --release --bin vulkan --features="vulkan"
  ```

Open a second terminal and run this to launch the web GUI:

```sh
trunk serve
  ```

Then,  go to the provided link `http://127.0.0.1:8080/` and visualise the spectrum. Once the GUI is open, you can use the sliders and radio buttons to adjust the gain, center frequency, sample rate, and display range (min/max dB) in real-time.