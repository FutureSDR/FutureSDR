# Vulkan Example

## Introduction

This example demonstrates how to leverage GPU acceleration in FutureSDR using the Vulkan API. It compares a standard CPU implementation of an exponential function with a Vulkan-powered GPU implementation.

## How It Works

The application creates two separate flowgraphs to process a large vector of random numbers:

1. CPU Path: Uses a standard `Apply` block to calculate the exponential on the host processor.
2. Vulkan Path: Offloads the calculation to the GPU.
    * Compute Shader: A GLSL program is compiled and run on the GPU's parallel cores.
    * GPU Memory: It uses `H2DWriter` (Host-to-Device) and `D2HReader` (Device-to-Host) to efficiently move data between RAM and VRAM.
    * Buffer Feedback `(src < snk)`: A feedback loop allows the source to reuse buffers as soon as the sink has finished with them, preventing unnecessary memory allocations.

## Requirements

To compile and run this example, you need:
* Vulkan SDK: Installed on your system. It provides the `shaderc` compiler.
* Vulkan-compatible drivers: Ensure that your GPU supports Vulkan.

## How to Run

By default, the example runs in Vulkan mode. You can explicitly switch to CPU mode to compare performance.

To run on the GPU:

```sh
cargo run --release
```

To run on the CPU:

```sh
cargo run --release -- --cpu
```
