WebGPU Example (wgpu)
=======================

## Introduction
This example demonstrates high-performance GPU computing integration within FutureSDR using the `wgpu` framework. It showcases how a signal processing flowgraph can leverage hardware acceleration to perform data-parallel operations. A key feature of this example is its cross-platform nature; thanks to the `wgpu` abstraction and WebAssembly (WASM) support, the same codebase runs natively on desktop APIs (Vulkan, Metal, DirectX) and within modern web browsers.

## How it works:
The flowgraph consists of the following blocks and components:

1. Vector Source: Generates a stream of random numbers on the CPU.
2. H2D Writer (Host-to-Device): An asynchronous buffer controller that manages the transfer of data chunks from system RAM to GPU VRAM.
3. Wgpu Block: The core processing unit that executes a WGSL (WebGPU Shading Language) compute shader. In this example, it performs a point-wise multiplication, scaling every input element by a factor of 12.
4. D2H Reader (Device-to-Host): A specialized buffer that pulls processed data back from the GPU memory to the CPU.
5. Vector Sink: Collects the final processed stream for validation.

As seen in the runtime logs, the system automatically handles data in optimized chunks (typically 4096 items or 16384 bytes per transaction), ensuring efficient use of the GPU's parallel architecture

## How to run:

* To run the example natively on your operating system:

```sh
cargo run --release
  ```

* To compile and serve the example for a web browser:

```bash
    # Add target
  $ rustup target add wasm32-unknown-unknown

    # Launch the server using trunk
  $ trunk serve 
``````
Then,  go to the link `http://localhost:8080`. Open the "Developer Tools" (F12) and check the "Console" tab. You will see the runtime logs and `INFO wgpu: data matches` confirming the GPU calculation succeeded within the browser environment.