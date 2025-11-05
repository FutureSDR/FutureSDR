# Runtime

<!--toc:start-->
- [Runtime](#runtime)
  - [Running a Flowgraph](#running-a-flowgraph)
  - [Starting a Flowgraph](#starting-a-flowgraph)
  - [Selecting a Scheduler](#selecting-a-scheduler)
  - [Runtime Handle](#runtime-handle)
<!--toc:end-->

A FutureSDR [Runtime](https://docs.rs/futuresdr/latest/futuresdr/runtime/struct.Runtime.html) has a [Scheduler](scheduler.md) associated with it and can run one or multiple [Flowgraphs](flowgraph.md).

## Running a Flowgraph

In the simplest case, you can construct a runtime with the default scheduler (Smol), execute a flowgraph and wait for its completion.

```rust
let mut fg = Flowgraph::new();
// set up the flowgraph

Runtime::new().run(fg)?;
```

The `run()` method takes ownership of the flowgraph and returns it after completion.

## Starting a Flowgraph

In most cases, you may want to start the flowgraph and continue instead of blocking and waiting for its completion, in which case one would use the `start` or `start_sync` methods.
The former is for use in async contexts.


```rust
let mut fg = Flowgraph::new();
// set up the flowgraph

let rt = Runtime::new();
let let (task_handle, flowgraph_handle) = rt.start_sync(fg)?;
```

The `task_handle` can be used to await completion of the flowgraph and getting ownership back afterwards (similar to `run()`).
The [`flowgraph_handle`](flowgraph.md#flowgraph-handle) can be used to interact with the
flowgraph (e.g., query its structure or send PMTs to blocks).

## Selecting a Scheduler

To use a different scheduler or change its configuration, you can specify it when constructing the runtime.

```rust
let mut fg = Flowgraph::new();
// set up the flowgraph

let rt = Runtime::with_scheduler(FlowScheduler::new());
rt.run(fg)?;
```

## Runtime Handle

It is possible to get a [RuntimeHandle](https://docs.rs/futuresdr/latest/futuresdr/runtime/struct.RuntimeHandle.html) to interact with the runtime from different contexts (e.g., other threads or closures).
The runtime handle can be passed around easily, since it is cloneable and implements the [`Send`](https://doc.rust-lang.org/std/marker/trait.Send.html) trait.
Using the handle, it is possible to launch flowgraphs or query the flowgraphs that are currently executed.

```rust
let rt = Runtime::new();
let handle = rt.handle();

async_io::block_on(async move {
    let mut fg = Flowgraph::new();
    // set up the flowgraph

    let _ = handle.start(fg).await;
});
```
