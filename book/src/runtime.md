# Runtime

A FutureSDR [Runtime](https://docs.rs/futuresdr/latest/futuresdr/runtime/struct.Runtime.html) owns a [Scheduler](scheduler.md) and starts one or more [Flowgraphs](flowgraph.md). On native targets, the runtime can start an integrated web server to serve a web UI and expose the [control port](flowgraph_interaction.md#rest-api) interface (i.e., a REST API) to interface with the runtime and the flowgraphs.

## Running a Flowgraph

The simplest way to execute a flowgraph is to construct a runtime, pass the flowgraph to `run()`, and block until it terminates.

```rust
let mut fg = Flowgraph::new();
// set up the flowgraph

let fg = Runtime::new().run(fg)?;
```

The `run()` method is a blocking call that takes ownership of the flowgraph and returns the finished flowgraph after all blocks have terminated. This is useful when you need to inspect blocks after execution, for example to read data or statistics.

In async code, use `run_async()` instead:

```rust
let mut fg = Flowgraph::new();
// set up the flowgraph

let fg = Runtime::new().run_async(fg).await?;
```

## Starting a Flowgraph

Use `start_async()` when the application should keep doing other work while the flowgraph is running. It returns once all blocks have initialized.

```rust
let mut fg = Flowgraph::new();
// set up the flowgraph

let rt = Runtime::new();
let running = rt.start_async(fg).await?;
```

On native targets, `start()` provides the same behavior from synchronous code:

```rust
let mut fg = Flowgraph::new();
// set up the flowgraph

let rt = Runtime::new();
let running = rt.start(fg)?;
```

Both methods return a [`RunningFlowgraph`](https://docs.rs/futuresdr/latest/futuresdr/runtime/struct.RunningFlowgraph.html). It combines the completion task with a [`FlowgraphHandle`](flowgraph.md#flowgraph-handle):

```rust
let running = rt.start(fg)?;

Runtime::block_on(async move {
    running.post(block_id, "handler_name", Pmt::U32(42)).await?;

    let fg = running.wait().await?;
    Ok::<_, Error>(fg)
})?;
```

Use `running.post()` and `running.call()` to interact with blocks, `running.wait().await` to wait for termination and recover the finished flowgraph, and `running.stop_and_wait().await` to request shutdown and then recover the finished flowgraph. Use `running.handle()` when you need to keep a clonable control handle. If you need to pass the two parts around separately, `running.split()` returns the `FlowgraphTask` and `FlowgraphHandle`.

## Selecting a Scheduler

To use a different scheduler or change its configuration, you can specify it when constructing the runtime.

```rust
let mut fg = Flowgraph::new();
// set up the flowgraph

let rt = Runtime::with_scheduler(FlowScheduler::new());
rt.run(fg)?;
```

## Runtime Handle

A [RuntimeHandle](https://docs.rs/futuresdr/latest/futuresdr/runtime/struct.RuntimeHandle.html) is a clonable control handle for the runtime. It is useful when other tasks, threads, web handlers, or callbacks need to start flowgraphs or query the flowgraphs registered with the runtime control plane.

```rust
let rt = Runtime::new();
let runtime_handle = rt.handle();

Runtime::block_on(async move {
    let mut fg = Flowgraph::new();
    // set up the flowgraph

    let running = runtime_handle.start(fg).await?;
    let flowgraph_handle = running.handle();
    let description = flowgraph_handle.describe().await?;

    Ok::<_, futuresdr::runtime::Error>(())
})?;
```

`RuntimeHandle::start()` returns a `RunningFlowgraph`. It also registers the flowgraph with the runtime control plane, so it remains available through `get_flowgraph()` and the control port.
