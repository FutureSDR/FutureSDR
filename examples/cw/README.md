CW (Morse Code) Transmitter Example (cw)
========================

## Introduction



## How it works:
The flowgraph consists of the following blocks:
* Vector Source: Takes your input string (e.g., "HELLO") and emits characters one by one.
* Morse Mapper: Text characters are translated into a series of Morse units (dots and dashes) by looking them up in a predefined alphabet list.
* 
* Sidetone Generator: A signal source produces a constant 700 Hz sine wave.


## How to run:
* You can run the transmitter directly from your terminal:

```sh
cargo run --release --example cw -- --message "{your_message}"
  ```

ıf you don't type a message, it sends "CQ CQ CQ FUTURESDR" by default.

* This example can also be used as a web application by running these commands:

 ```bash
  $ cargo install trunk

  $ trunk serve index.html
  ``````

Then, open `http://localhost:8080` in your browser. You will see a text input and a `Start` button. Type our message in the vacancy and press the button to trigger the Morse code audio.

