# WebAssembly Example

## Introduction

This example demonstrates how to run a simple flowgraph on both desktop and in web browsers.

## How It Works

The flowgraph consists of the following blocks:
* Source: Creates 100,000 random numbers.
* Apply: Multiplies each number by 12.0.
* Sink: Checks if the results are correct.

## How to Run

To run the example on desktop, go to the example directory and run:

```sh
cargo run --release
```

When the flowgraph finishes, you will see the following logs:

```text
INFO main futuresdr::runtime::runtime: after init in runtime
INFO main futuresdr::runtime::runtime: runtime constructed
INFO main wasm: data matches
```

To run the WASM version, use the following commands:

```sh
cargo install trunk

trunk serve
```

Then visit [`http://localhost:8080`](http://localhost:8080).

The terminal will show Trunk build logs. The browser console will show the flowgraph's log output.
