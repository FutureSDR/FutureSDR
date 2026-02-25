Macros Example (macros)
=======================

## Introduction

This example demonstrates the usage of `connect!` macro in FutureSDR. It serves as a syntax guide for building and connecting different types of data paths within a flowgraph.

## How it works:
The flowgraph defines three independent execution paths that are launched simultaneously as concurrent asynchronous tasks when the runtime starts:

1. Stream Path (`>`): A continuous data pipeline where integer samples flow from a `VectorSource` through multiple `Copy` blocks into a `NullSink`.
2. Message Path (`|`): An asynchronous message chain that handles 20 string messages (sent every 100ms) passing through `MessageCopy` blocks to a `MessageSink` and a custom `Handler`.
3. Standalone Block: A `Dummy` block with no connections that starts and terminates immediately, demonstrating that blocks can exist in the flowgraph without being part of a chain.

The runtime manages these paths in parallel and terminates after all tasks have completed their work.

## How to run:
Since this example is designed to show syntax and flowgraph construction, it produces no external files or audio. It aims to verify that the flowgraph compiles and runs correctly. It can be checked by running this:

```sh
cargo run --release
  ```