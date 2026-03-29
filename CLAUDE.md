# AGENTS.md

This file provides guidance for AI agents when working with code in this repository.

## Overview

FutureSDR is an experimental asynchronous SDR (Software Defined Radio) runtime for heterogeneous architectures. It supports stream-based and message-based data processing through **Blocks** that combine into **Flowgraphs** executed by a **Runtime** and **Scheduler**.

FutureSDR implements a flexible pipeline computation model that forms a directed acyclic graph. Every block can have an arbitrary number of inputs and outputs. The runtime can be extended with custom buffer and scheduler implementations.

FutureSDR borrows ideas from the actor model, where a block is an actor that reads from a mpsc channel. Through the channel, the block is either notified that it should run again or gets BlockMessages that provide more metadata to the message, e.g., call a message handler with this data argument.

## Project Goals

- FutureSDR is meant to experiment with novel concepts, API stability is not a goal right now. The main question is: how could an SDR framework in Rust look like?
- FutureSDR code should be minimal, elegant, performant, and follow Rust best-practices.
- It is more important that the user facing API (instantiate blocks, create a flowgraph, run the flowgraph) is ergonomic than the developer API (implement blocks, custom schedulers, or custom buffers).
- It is important that the core runtime code (in `src/runtime`) that glues everything together is easy to comprehend and minimal.
- Specific implementations that contain complexity (e.g., a buffer implementation, a scheduler implementation) can be highly optimized and complexity is ok.

## Build & Test Commands

The root crate uses Rust 2024 edition and currently declares `rust-version = "1.89"`. Build and test commands work on stable Rust, but repository formatting uses `cargo +nightly fmt`, and several Leptos-based frontend crates enable Leptos' `nightly` feature.

The root workspace contains `.`, `crates/futuredsp`, `crates/macros`, and `crates/types`. `crates/prophecy`, `crates/remote`, every directory under `examples/`, and every directory under `perf/` are independent Cargo workspaces.

```sh
# Build main crate (default features)
cargo build

# Build with specific features
cargo build --features=burn,vulkan,zeromq,audio,flow_scheduler,seify_dummy,wgpu

# Run all tests (main workspace)
cargo test --all-targets --workspace --features=vulkan,zeromq,audio,flow_scheduler,seify_dummy,soapy,wgpu,zynq

# Run a single test
cargo test --test flowgraph
cargo test --test apply

# Run tests for a sub-crate
cargo test --all-targets --manifest-path=crates/futuredsp/Cargo.toml
cargo test --all-targets --all-features --manifest-path=crates/types/Cargo.toml

# Lint (matches the root check script)
cargo clippy --all-targets --workspace --features=burn,vulkan,zeromq,audio,flow_scheduler,soapy,zynq,wgpu,seify_dummy -- -D warnings

# Format (repository convention uses nightly rustfmt)
cargo +nightly fmt --all

# Format check
cargo +nightly fmt --all -- --check
```

### Examples, Perf, and independent crates

`crates/prophecy`, `crates/remote`, and each directory under `examples/` and `perf/` is an independent Cargo workspace. Build/test them with `--manifest-path`:

```sh
cargo build --manifest-path=examples/wlan/Cargo.toml
cargo test --all-targets --manifest-path=examples/wlan/Cargo.toml
cargo clippy --all-targets --manifest-path=perf/burn/Cargo.toml -- -D warnings
cargo test --all-targets --manifest-path=crates/remote/Cargo.toml
```

### WASM builds

```sh
rustup target add wasm32-unknown-unknown
cargo clippy --lib --workspace --features=burn,audio,seify_dummy,wgpu --target wasm32-unknown-unknown -- -D warnings
```

### Prophecy GUI (web frontend)

Located at `crates/prophecy/`, built with [Trunk](https://trunkrs.dev):

```sh
cd crates/prophecy
trunk build --release   # output in dist/
trunk serve             # dev server
```

Served automatically at `http://localhost:1337/` when running any FutureSDR application.

## Architecture

### Core Concepts

**Block** — the fundamental processing unit. Every block implements the `Kernel` trait (`src/runtime/kernel.rs`):
- `init()` — called once at startup
- `work()` — called repeatedly to process stream data; sets `WorkIo` flags to signal state
- `deinit()` — called once at shutdown

Blocks are created using the `#[derive(Block)]` proc macro (from `crates/macros/`) which generates the `KernelInterface` impl. The macro derives stream/message port declarations from annotated struct fields.

**Flowgraph** (`src/runtime/flowgraph.rs`) — a directed graph of blocks connected via stream ports or message ports. Built using the `connect!` macro:
```rust
connect!(fg,
    src > head > snk;               // stream connection (default "out"/"in" ports)
    src."custom out" > snk;         // named ports
    producer | consumer;            // message connection
);
```
`connect!` both adds blocks to the flowgraph and wires their ports.

**Runtime** — drives a Flowgraph on a Scheduler. After construction, blocks receive `Initialize`, then loop in `work()` until done.

**Scheduler** (`src/runtime/scheduler/`) — pluggable execution engines:
- `SmolScheduler` — default, async executor (non-WASM)
- `FlowScheduler` — feature-gated (`flow_scheduler`), specialized scheduler
- `WasmScheduler` — for WASM targets

**MegaBlock** (`src/runtime/megablock.rs`) — composes multiple blocks into a reusable sub-graph with typed stream ports exposed externally.

### Buffer System (`src/runtime/buffer/`)

Buffers are the transport layer between blocks. Implementations:
- `circular` — double-mapped circular buffer; default for CPU-to-CPU on non-WASM (maps to `DefaultCpuReader/Writer`)
- `slab` — slab buffer; default on WASM
- `circuit` — in-place circuit buffer (avoids copies)
- `vulkan` — GPU memory via Vulkan API (feature: `vulkan`)
- `wgpu` — GPU memory via WGPU (feature: `wgpu`)
- `burn` — for Burn ML framework (feature: `burn`)
- `zynq` — Xilinx Zynq FPGA DMA (feature: `zynq`, Linux only)

Buffer traits: `BufferReader` / `BufferWriter` (generic), `CpuBufferReader` / `CpuBufferWriter` (CPU-specific with `slice()`/`consume()`/`produce()`), `InplaceReader` / `InplaceWriter` (in-place).

### Message Passing

Blocks communicate via typed `Pmt` (Polymorphic Message Type) values from `crates/types/`. Message ports use `#[message_handler]` attribute on async handler methods. The `MessageOutputs` / `BlockInbox` types handle routing between blocks.

### Crate Structure

| Path | Crate | Purpose |
|------|-------|---------|
| `.` | `futuresdr` | Main runtime, blocks, schedulers, buffers |
| `crates/futuredsp` | `futuredsp` | DSP algorithms (FIR, resampling, etc.) |
| `crates/macros` | `futuresdr-macros` | Proc macros: `#[derive(Block)]`, `connect!`, `#[message_handler]` |
| `crates/types` | `futuresdr-types` | `Pmt` and shared serializable types |
| `crates/prophecy` | `prophecy` | Web GUI (Leptos + WASM, served by ControlPort) |
| `crates/remote` | `futuresdr-remote` | Remote control client library |
| `examples/` | — | Independent example workspaces |
| `perf/` | — | Independent performance benchmark workspaces |

### Control Port

`src/runtime/ctrl_port.rs` exposes an Axum-based REST API (default bind `127.0.0.1:1337`) for runtime introspection and message injection. The Prophecy GUI connects to this API.

### Testing

Integration tests live in `tests/`. The `Mocker` module (`src/runtime/mocker.rs`) is the preferred way to unit-test a single block in isolation without a full runtime on native targets.

```rust
let mut mocker = Mocker::new(MyBlock::new());
mocker.input().set(vec![1.0f32, 2.0, 3.0]);
mocker.output().reserve(3);
mocker.run();
let (output, _tags) = mocker.output().get();
```

## Key Features / Feature Flags

- `vulkan` — Vulkan GPU buffer support
- `wgpu` — WGPU GPU buffer support
- `burn` — Burn ML framework integration
- `flow_scheduler` — FlowScheduler (requires `spin`)
- `audio` — Audio blocks (cpal/rodio/hound)
- `zeromq` — ZeroMQ source/sink blocks
- `seify` / `seify_dummy` — SDR hardware abstraction (RTL-SDR, HackRF, SoapySDR, etc.)
- `zynq` — Xilinx Zynq FPGA DMA support (Linux only)
