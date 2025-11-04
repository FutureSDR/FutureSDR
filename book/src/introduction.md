# Introduction

FutureSDR is a novel SDR runtime implemented in Rust.

## Main Features

- **Platform support:** FutureSDR runs on Linux, Windows, macOS, Android, and on the web.
- **Accelerators:** FutureSDR supports efficient accelerator integration through custom buffers that provide direct access to accelerator memory (e.g., DMA buffers, GPU staging buffers, machine learning tensors).
  Developers can implement their own buffers or use existing ones for Xilinx Zynq DMA, Vulkan GPU, and [Burn](https://burn.dev), a Rust machine learning framework.
- **Custom Schedulers:** FutureSDR uses an async runtime that schedules data processing workloads as tasks in user space. This allows plugging in different scheduling strategies.

## Core Concepts

While the technical realization of FutureSDR is different from existing frameworks, the core abstractions are similar.
It supports *Blocks* that implement stream-based or message-based data processing.
These blocks can be combined to a *Flowgraph* and launched on a *Runtime* that is driven by a *Scheduler*.
