Vulkan Example (vulkan)
=======================

## Introduction

This example demonstrates how to leverage GPU acceleration within FutureSDR using the Vulkan API. It compares a standard CPU implementation of an exponential function against a Vulkan-powered GPU implementation.

## How it works:
The application creates two separate flowgraphs to process a large vector of random numbers:

1. CPU Path: Uses a standard `Apply` block to calculate exponential using the host processor.
2. Vulkan Path: Offloads the calculation to the GPU.
    * Compute Shader: A GLSL program is compiled and run on the GPU's parallel cores.
    * Zero-Copy Memory: It uses `H2DWriter` (Host-to-Device) and `D2HReader` (Device-to-Host) to efficiently move data between RAM and VRAM.
    * Buffer Feedback `(src < snk)`: A feedback loop allows the source to reuse buffers as soon as the sink is finished with them, preventing unnecessary memory allocations.

## Requirements
To compile and run this example, you need:
* Vulkan SDK: Installed on your system (provides the `shaderc` compiler).
* Vulkan Compatible Drivers: Ensure your GPU supports Vulkan.

## How to run:
By default, the example runs in Vulkan mode. You can explicitly switch to CPU mode to compare performance.

* To run with GPU:

```sh
cargo run --release
  ```

* To run with CPU:

```sh
cargo run --release -- --cpu
  ```