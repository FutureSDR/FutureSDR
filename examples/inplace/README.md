# In-Place Buffer Example

## Introduction

This example demonstrates how to use in-place, zero-copy buffers in FutureSDR. It runs three flowgraphs:

1. An out-of-place version that uses standard CPU buffers.
2. An in-place version that reuses the same buffers across the pipeline.
3. A hybrid version in which standard CPU sources and sinks interface with an intermediate in-place stage.

The hybrid setup works because the in-place buffers also implement the `CpuBufferReader` and `CpuBufferWriter` traits, which means they can also be used like normal CPU buffers.

## How It Works

Each flowgraph processes the same input vector of integers from `0` to `999_998` and increments every item by `1`.

The example contains the following components:

* `VectorSource`: Produces a vector of `i32` samples.
* `Apply`: Reads a buffer, increments each item in place with `wrapping_add(1)`, and forwards the same buffer.
* `VectorSink`: Collects the processed output and verifies the result.

The three runs differ as follows:

1. Out-of-place: Uses the standard FutureSDR `VectorSource`, `Apply`, and `VectorSink` blocks.
2. In-place: Uses custom `VectorSource`, `Apply`, and `VectorSink` blocks built on `InplaceReader` and `InplaceWriter`.
3. Hybrid: Uses the standard FutureSDR `VectorSource` and `VectorSink` together with the custom in-place `Apply` block.

For the in-place and hybrid variants, the source injects reusable buffers into the circuit. The sink then returns consumed buffers so they can be reused instead of reallocated.

Each run measures and prints its execution time:

* `in-place took ...`
* `hybrid took ...`
* `out-of-place took ...`

## How to Run

Go to the example directory and run:

```sh
cargo run --release
```

The program executes all three flowgraphs in sequence, prints their timings, and checks that every output item matches the expected incremented value.
