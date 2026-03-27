# CW (Morse Code) Transmitter

## Introduction

This example demonstrates how to build a Continuous Wave (CW) transmitter that converts text input into audible Morse code. It can run both as a native terminal application and in a web browser using WebAssembly (WASM).

## How It Works

The flowgraph consists of the following blocks:
* Vector Source: Takes your input string (for example, "HELLO") and emits characters one by one.
* Morse Mapper: Translates text characters into a series of Morse units (dots and dashes) by looking them up in a predefined alphabet list.
* Timing Generator: Converts each Morse symbol into a stream of 1.0 (sound on) and 0.0 (sound off) values based on `DOT_LENGTH`.
  - Dot: 1 unit of sound, followed by 1 unit of silence.
  - Dash: 3 units of sound, followed by 1 unit of silence.
* Sidetone Generator: A signal source produces a constant 700 Hz sine wave.
* Switch: Multiplies the carrier sine wave by the 1.0/0.0 stream, turning the sound on and off to create the Morse code signal (on-off keying).
* Audio Sink: Sends the final "chopped" sine wave to your speakers at a 48 kHz sample rate.

## How to Run

You can run the transmitter directly from your terminal:

```sh
cargo run --release -- --message "{your_message}"
```

If you don't type a message, it sends "CQ CQ CQ FUTURESDR" by default.

This example can also be used as a web application by running:

```sh
trunk serve
```

Then open [`http://localhost:8080`](http://localhost:8080) in your browser. You will see a text input and a `Start` button. Type your message into the input field and press the button to trigger the Morse code audio.
