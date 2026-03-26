# Macro Usage Example

## Introduction

This example demonstrates the use of the `connect!` macro in FutureSDR. The macro is used to connect different port types within a flowgraph.

## How It Works

FutureSDR differentiates between *stream* and *message* ports. Both can be connected using the `connect!` macro.
The example shows:

1. Stream Ports (`>`): A continuous data pipeline where integer samples flow from a `VectorSource` through multiple `Copy` blocks into a `NullSink`.
2. Message Ports (`|`): An asynchronous message chain that handles 20 string messages, sent every 100 ms, passing through `MessageCopy` blocks to a `MessageSink` and a custom `Handler`.
3. Standalone Block: A `Dummy` block with no connections that starts and terminates immediately, demonstrating that blocks can exist in the flowgraph without being part of a chain.

The runtime manages these paths in parallel and terminates after all tasks have completed their work.

## How to Run

Since this example is designed to show syntax and flowgraph construction, it produces no external files or audio. It is intended to verify that the flowgraph compiles and runs correctly. You can check it by running:

```sh
cargo run --release
```
