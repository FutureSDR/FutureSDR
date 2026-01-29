WebAssembly Example (wasm)
========================

## Introduction

This example demonstrates running a simple flowgraph on both desktop and web browsers.

## How it works:
The flowgraph consists of the following blocks:
* Source: Creates 100,000 random numbers.
* Apply: Multiplies each number by 12.0.
* Sink: Checks if the results are correct.

## How to run:
* To run the example on desktop, go to the directory of this example and run it:

```sh
cargo run --release
  ```

When the flowgraph finishes, you will see the following logs:

```text
INFO main futuresdr::runtime::runtime: after init in runtime
INFO main futuresdr::runtime::runtime: runtime constructed
INFO main wasm: data matches
  ```

* To use the WASM, run these commands:

 ```bash
  $ cargo install trunk

  $ trunk serve 
  ``````
  Then,  go to the link `http://localhost:8080`.

The terminal will show build logs of Trunk.
The web page will display a static message, which directs you to more interesting WASM applications.