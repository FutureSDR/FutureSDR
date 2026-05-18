# Scheduler

Schedulers are responsible for executing the blocks in a [Flowgraph](flowgraph.md) and potentially other async tasks. A scheduler decides where block tasks run, how general async tasks are spawned, and how blocking work is handled.

Most applications should use the default scheduler through `Runtime::new()`. Select a scheduler explicitly only when you need to configure the native executor or when benchmarking shows that a different scheduler improves a specific flowgraph.

## Smol

`SmolScheduler` is the default scheduler on native targets and is the recommended scheduler for general use. It is based on the `smol` async runtime and runs block tasks on a pool of executor threads.

The default runtime uses `SmolScheduler::default()`, which creates one worker per detected CPU core and does not pin workers to cores:

```rust
use futuresdr::prelude::*;

let mut fg = Flowgraph::new();
// set up the flowgraph

let fg = Runtime::new().run(fg)?;
```

Instantiate it explicitly when you want to configure the number of workers or CPU pinning:

```rust
use futuresdr::prelude::*;
use futuresdr::runtime::scheduler::SmolScheduler;

let mut fg = Flowgraph::new();
// set up the flowgraph

let scheduler = SmolScheduler::new(2, false);
let fg = Runtime::with_scheduler(scheduler).run(fg)?;
```

The first argument is the number of executor threads. The second argument enables CPU pinning. When pinning is enabled, workers are pinned to the detected CPU cores in order:

```rust
use futuresdr::prelude::*;
use futuresdr::runtime::scheduler::SmolScheduler;

let scheduler = SmolScheduler::new(4, true);
let rt = Runtime::with_scheduler(scheduler);
```

## Flow

`FlowScheduler` is a custom native scheduler for more controlled execution. It is available with the `flow_scheduler` feature:

```sh
cargo run --features=flow_scheduler --example minimal
```

Use `FlowScheduler::new()` to let the scheduler assign all blocks to worker-local queues with its default deterministic mapper:

```rust
use futuresdr::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;

let mut fg = Flowgraph::new();
// set up the flowgraph

let scheduler = FlowScheduler::new();
let fg = Runtime::with_scheduler(scheduler).run(fg)?;
```

With `FlowScheduler::new()`, blocks are not placed in the global queue. Each block is mapped to one worker queue based on its block ID, the number of blocks, and the number of workers. Each worker calls its local blocks round-robin. General async tasks spawned on the scheduler, and local tasks that overflow a worker queue, use a global queue that workers poll when their local work is idle.

For explicit control, use `FlowScheduler::with_pinned_blocks()` to assign selected blocks to fixed workers. The outer vector index is the worker index, and each inner vector lists the block IDs assigned to that worker in initial queue order. Blocks that are not listed still use the default deterministic mapper:

```rust
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;

let mut fg = Flowgraph::new();

let src = NullSource::<f32>::new();
let head = Head::<f32>::new(1_000_000);
let snk = NullSink::<f32>::new();

connect!(fg, src > head > snk);

let scheduler = FlowScheduler::with_pinned_blocks(vec![
    vec![src.id(), head.id()],
    vec![snk.id()],
]);

Runtime::with_scheduler(scheduler).run(fg)?;
```

Benchmark before switching to the Flow Scheduler. Its deterministic mapping can help with some pipelines, but it is not guaranteed to outperform the default scheduler.

## WebAssembly

`WasmScheduler` is selected by `Runtime::new()` automatically when compiling for `wasm32` and starts one web worker by default. Use `WasmScheduler::new(n)` to run normal blocks on a worker pool.

Some browser APIs, including CPAL's WebAudio output backend, must be created and owned on the browser main thread. For those cases, use `Runtime::with_scheduler(futuresdr::runtime::scheduler::wasm::WasmMainScheduler::new())`. This keeps the flowgraph in FutureSDR but runs normal blocks on the UI thread, so it should only be used for light flowgraphs or main-thread-only browser APIs.

Local domains are available on WASM as well. A local domain creates a dedicated web worker and receives the closure that instantiates each local block, mirroring the native local-domain model. FutureSDR uses one worker script path for both scheduler workers and local-domain workers. The default is `./futuresdr-wasm-scheduler-worker.js`; configure a different path with `futuresdr::runtime::scheduler::wasm::set_worker_script(path)` before creating schedulers or local domains.

With Trunk, the simplest setup is to give the Rust output a fixed target name and copy the stock worker template. For a `cdylib` app:

```html
<link data-trunk rel="rust" data-target-name="futuresdr_app" data-weak-refs data-reference-types />
<link data-trunk rel="copy-file" href="assets/futuresdr-wasm-scheduler-worker.js" />
```

For a bin target, add a `[[bin]]` alias named `futuresdr_app` in `Cargo.toml` and select it from Trunk:

```toml
[[bin]]
name = "futuresdr_app"
path = "src/bin/app.rs"
```

```html
<link data-trunk rel="rust" data-bin="futuresdr_app" data-weak-refs data-reference-types />
<link data-trunk rel="copy-file" href="assets/futuresdr-wasm-scheduler-worker.js" />
```

The worker template imports `./futuresdr_app.js`, initializes the module/memory from the init message, and dispatches both `futuresdr-wasm-scheduler-init` and `futuresdr-wasm-local-domain-init` messages. See `examples/wasm-threaded/assets/futuresdr-wasm-scheduler-worker.js` for the template. If a flowgraph is started from another web worker, give that worker target the `futuresdr_app` name/alias and use the same template there as well.

```rust
use futuresdr::prelude::*;

let mut fg = Flowgraph::new();
// set up the flowgraph

let fg = Runtime::new().run_async(fg).await?;
```
