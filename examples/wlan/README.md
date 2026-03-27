# Wi-Fi Transceiver

## Introduction

This project provides a complete IEEE 802.11 a/g/p (Wi-Fi) implementation. It supports encoding and decoding standard-compliant Wi-Fi frames, enabling communication between SDR devices or within a simulated environment. It can also be used to observe Wi-Fi traffic in the surrounding environment. The implementation covers the physical (PHY) layer, including synchronization, equalization, and MAC-to-PHY mapping.

The project features three primary operating modes and various visualization tools for real-time frame analysis and signal quality monitoring.

## Loopback Mode

Loopback mode is an internal simulation that connects the transmitter and receiver within a single flowgraph. It adds simulated Gaussian noise to the signal, making it ideal for testing the full transceiver chain without physical SDR hardware.

```sh
cargo run --release --bin loopback
```

## Transmitter

In this mode, the application generates 802.11 frames and transmits them through a supported SDR device. It periodically sends a "FutureSDR" payload with incrementing sequence numbers.

```sh
cargo run --release --bin tx -- --gain {value} --channel {channel_number}
```

## Receiver

Receiver mode captures RF signals from an SDR, performs synchronization, and decodes the frames. It is designed to work alongside external tools for packet inspection.

```sh
cargo run --release --bin rx -- --gain {value} --channel {channel_number}
```

## Visualization and Analysis

This implementation provides multiple ways to observe and analyze Wi-Fi traffic in real time.

### Web Interface

You can access a web-based dashboard that displays the constellation diagram. This is useful for monitoring modulation quality and link quality.

First, run this command in another terminal while the receiver is running:

```sh
trunk serve
```

Then, navigate to [`http://127.0.0.1:8080/`](http://127.0.0.1:8080/) in your web browser.

### Python Packet Parser

The decoded frames are streamed over UDP on port 55555. The provided Python script, which uses the [Scapy](https://scapy.net/) library, decodes and displays frame headers, MAC addresses, SSIDs for management frames, and encryption types for data frames in real time.

In a second terminal, run this command:

```sh
python3 parse.py
```

Demo Video: [FutureSDR WLAN Receiver (20MHz, 802.11a)](https://www.youtube.com/watch?v=aOqEaAKVsbY)

### Wireshark

The receiver also supports RadioTap encapsulation and streams the output to port 55556. This allows you to use Wireshark for advanced protocol analysis, including observing management beacons, control frames, and encrypted data traffic (CCMP).

Demo Video: [FutureSDR WLAN GUI](https://www.youtube.com/watch?v=Pj3f6-p0yG0)
