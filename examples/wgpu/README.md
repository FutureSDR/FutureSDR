# WebGPU Example

## Introduction

This example demonstrates GPU computing integration in FutureSDR using the [wgpu](https://wgpu.rs/) framework. It showcases how a signal processing flowgraph can leverage hardware acceleration to perform data-parallel operations. A key feature of this example is its cross-platform nature: thanks to the `wgpu` abstraction and WebAssembly (WASM) support, the same codebase runs natively on desktop APIs (Vulkan, Metal, DirectX) and in modern web browsers.

## How It Works

The flowgraph consists of the following blocks and components:

1. Vector Source: Generates a stream of random numbers on the CPU.
2. H2D Writer (Host-to-Device): An asynchronous buffer controller that manages the transfer of data chunks from system RAM to GPU VRAM.
3. Wgpu Block: The core processing unit that executes a WGSL (WebGPU Shading Language) compute shader. In this example, it performs a point-wise multiplication, scaling every input element by a factor of 12.
4. D2H Reader (Device-to-Host): A specialized buffer that pulls processed data back from the GPU memory to the CPU.
5. Vector Sink: Collects the final processed stream for validation.


## How to Run

To run the example natively on your operating system:

```sh
cargo run --release
```


To compile and serve the example for a web browser:

```sh
# Add target
rustup target add wasm32-unknown-unknown

# Launch the server using trunk
trunk serve

```
Then go to [`http://localhost:8080`](http://localhost:8080). Open the developer tools (F12) and check the Console tab. You will see the runtime logs and `INFO wgpu: data matches`, confirming that the GPU calculation succeeded in the browser environment.
