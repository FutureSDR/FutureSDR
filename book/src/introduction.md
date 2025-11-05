# Introduction

FutureSDR is a software-defined radio (SDR) runtime written in Rust with a
focus on portability, performance, and developer ergonomics.

## Main Features

- **Platform support:** FutureSDR runs on Linux, Windows, macOS, Android, and on the
  web. Support for both native and browser targets allows you to reuse the same
  signal-processing code across desktop, embedded, and WebAssembly deployments.
- **Accelerators:** FutureSDR integrates with accelerators through custom buffers
  that provide direct access to accelerator memory (e.g., DMA buffers, GPU
  staging buffers, machine-learning tensors). Developers can implement their own
  buffers or reuse existing ones for Xilinx Zynq DMA, Vulkan GPU, and
  [Burn](https://burn.dev), a Rust machine-learning framework.
- **Custom Schedulers:** FutureSDR uses an async runtime that schedules
  data-processing workloads as user-space tasks. This architecture lets you plug
  in different scheduling strategies to match your latency and throughput goals.

## Core Concepts

While FutureSDR’s implementation differs from other SDR frameworks, the core
abstractions remain familiar. It supports *Blocks* that implement stream-based
or message-based data processing. These blocks can be combined into a
*Flowgraph* and launched on a *Runtime* that is driven by a *Scheduler*.

