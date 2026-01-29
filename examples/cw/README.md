CW (Morse Code) Transmitter Example (cw)
========================

## Introduction

This example demonstrates how to build a Continuous Wave (CW) transmitter that converts text input into audible Morse code. It offers an alternative to run the same application on both a native terminal and a web browser using WebAssembly (WASM).

## How it works:
The flowgraph consists of the following blocks:
* Vector Source: Takes your input string (e.g., "HELLO") and emits characters one by one.
* Morse Mapper: Text characters are translated into a series of Morse units (dots and dashes) by looking them up in a predefined alphabet list.
* Timing Generator: Each Morse symbol is converted into a stream of 1.0 (sound on) and 0.0 (sound off) based on the `DOT_LENGTH`.
  - Dot: 1 unit of sound, 1 unit of silence.
  - Dash: 3 units of sound, 1 unit of silence.
* Sidetone Generator: A signal source produces a constant 700 Hz sine wave.
* The Switch: Multiplies the carrier sine wave by the 1.0/0.0 stream, turning the sound on and off to create the Morse code signal (On-Off Keying).
* Sends the final "chopped" sine wave to your speakers at a 48 kHz sample rate.


## How to run:
* You can run the transmitter directly from your terminal:

```sh
cargo run --release -- --message "{your_message}"
  ```

If you don't type a message, it sends "CQ CQ CQ FUTURESDR" by default.

* This example can also be used as a web application by running these commands:

 ```bash
  $ cargo install trunk

  $ trunk serve index.html
  ``````

Then, open `http://localhost:8080` in your browser. You will see a text input and a `Start` button. Type our message in the vacancy and press the button to trigger the Morse code audio.

