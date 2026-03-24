WiFi Example (wlan)
========================

## Introduction

This project provides a complete IEEE 802.11 a/g/p (WiFi) implementation. It supports encoding and decoding of standard-compliant WiFi frames, enabling communication between SDR devices or within a simulated environment. Moreover, the WiFi traffic in the environment can be observed using this application. The implementation covers the physical (PHY) layer, including synchronization, equalization, and MAC-to-PHY mapping.

The project features three primary operation modes and various visualization tools for real-time frame analysis and signal quality monitoring.

## 1. Loopback Mode (loopback)
The loopback mode is an internal simulation that connects the transmitter and receiver within a single flowgraph. It adds simulated Gaussian noise to the signal, making it ideal for testing the full transceiver chain without physical SDR hardware.

```sh
cargo run --release --bin loopback
  ```

## 2. Transmitter (tx)
In this mode, the application generates 802.11 frames and transmits them via a supported SDR device. It periodically sends a "FutureSDR" payload with incrementing sequence numbers.

```sh
cargo run --release --bin tx -- --gain {value} --channel {channel_number}
  ```

## 3. Receiver (rx)
The receiver mode captures physical signals from an SDR, performs synchronization, and decodes the frames. It is designed to work alongside external tools for deep packet inspection.

```sh
cargo run --release --bin rx -- --gain {value} --channel {channel_number}
  ```

## Visualization and Analysis
This implementation provides multiple ways to observe and analyze the WiFi traffic in real-time.

* Web Interface:
You can access a web-based dashboard that displays the constellation diagram. This is useful for monitoring signal modulation and link quality.

First, run this command in another terminal while the receiver is running:

```sh
trunk serve
  ```

Then, navigate to `http://127.0.0.1:8080/` in your web browser.

* Python Packet Parser:
The decoded frames are streamed via UDP (port 55555). The provided Python script (utilizing the `scapy` library) decodes and displays frame headers, MAC addresses, SSIDs for management frames, and encryption types for data frames in real-time.

In a second terminal, run this command:

```sh
python3 parse.py
  ```

Demo Video: [FutureSDR WLAN Receiver (20MHz, 802.11a)](https://www.youtube.com/watch?v=aOqEaAKVsbY)

* Wireshark:
The receiver also supports RadioTap encapsulation, streaming the output to port 55556. This allows you to use Wireshark for advanced protocol analysis, observing management beacons, control frames, and encrypted data traffic (CCMP).

Demo Video: [FutureSDR WLAN GUI](https://www.youtube.com/watch?v=Pj3f6-p0yG0)