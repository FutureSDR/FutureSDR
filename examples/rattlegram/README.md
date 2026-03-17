Rattlegram Example (rattlegram)
========================

## Introduction

Rattlegram is a protocol for sending short text messages over analog audio channels using Coded Orthogonal Frequency-Division Multiplexing (COFDM). It converts digital text into audible tones that can be transmitted via speakers or other voice-grade channels, acting as a form of "SMS via audio" for environments without internet or cellular service. By bridging the gap between digital data and analog sound, it enables text communication across almost any device that can play or record audio.

This project provides an implementation of both a transmitter and a receiver for the Rattlegram protocol. The project is divided into four operation modes, allowing for flexible use across different platforms.

## 1. Transmitter (tx)

Text messages are converted into Rattlegram-compatible audio signals. The signal can be either broadcasted directly or saved to a file.

* To broadcast via speakers:

```sh
cargo run --release --bin tx -- --payload {your_message} --call-sign {call_sign}
  ```

* To save the signal to a file:

```sh
cargo run --release --bin tx -- --payload {your_message} --call-sign {call_sign} --file {file_name}.{file_type}
  ```

## 2. Receiver (rx)

It decodes Rattlegram signals from either a live microphone or a recorded file.

* To receive signal from microphone:

```sh
cargo run --release --bin rx
  ```

* To decode a recorded signal:
```sh
cargo run --release --bin rx -- --file {file_name}.{file_type}
  ```

Note that this implementation is compatible with the Rattlegram mobile application. Alternatively, the transmitter and receiver can be run on two different terminals simultaneously. 

## 3. Tranceiver (trx)

This is a combined mode that supports both transmitting and receiving in a single session.

```sh
cargo run --release --bin trx
  ```

The application has an interactive terminal interface, allowing you to enter new messages and monitor the content of incoming signals simultaneously

## 4. Web Interface

This project also provides a graphical transceiver that runs in a web browser using WebAssembly.

First, run this command:

```sh
trunk serve
  ```

Then, navigate to `http://127.0.0.1:8080/` in your web browser.

* TX: Fill in the `Call Sign` and `Payload` fields and click `TX Message`.
* RX: `Click Start RX` to enable microphone access. Decoded messages and call signs appear in a real-time list on the page.