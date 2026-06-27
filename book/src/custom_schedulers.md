# Custom Schedulers

Schedulers execute normal flowgraph block tasks and async tasks spawned through the runtime. Most applications should use `Runtime::new()` and the default `SmolScheduler`; write a scheduler only when you are experimenting with placement, latency, or executor integration.

The scheduler trait is:

```rust
pub trait Scheduler: Clone + Send + 'static {
    fn start_normal_domain(&self, spec: NormalDomainSpec) -> Result<NormalRunningDomain>;

    fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T>;
}
```

`start_normal_domain()` receives the normal send-capable blocks and domain topology. It usually spawns each block, calls `block.run(main_channel).await`, and returns task handles through `NormalRunningDomain`. The runtime waits for those tasks and restores the finished block objects into the returned flowgraph.

Local domains are not started by the normal scheduler. The runtime activates the already-created local-domain thread or worker directly, constructs the flowgraph's local scheduler type with `Default` inside that domain, and lets it orchestrate non-`Send` block tasks through `LocalScheduler::run_local_domain()`.

`spawn()` runs general sendable async tasks on the scheduler. `Runtime::spawn()`, `Runtime::spawn_background()`, and control-plane internals use this method.

## Normal vs Local Work

Schedulers manage the implicit normal domain, which contains send-capable block tasks. Local domains are created by the flowgraph for:

- blocks added through `Flowgraph::with_local_domain()`,
- blocks marked with `#[blocking]`.

Blocking or thread-affine work should be placed in a local domain instead of being hidden inside the normal scheduler. A local domain can select a local scheduler type with `fg.local_domain_with_scheduler::<MyLocalScheduler>()`; `fg.local_domain()` uses the built-in basic local scheduler.

Custom local schedulers implement `LocalScheduler`. The low-level `run()` hook drives the local non-`Send` executor, while `run_local_domain()` receives a `LocalDomainRunSpec` with opaque primitives for inspecting topology, taking runnable local blocks, handling domain events, stopping blocks, and restoring stopped block state. Most implementations should customize `spawn()` / `run()` and delegate to `BasicLocalScheduler::run_basic(self, spec)`.

## Starting Point

Use the existing schedulers as templates:

- `SmolScheduler` is a compact general-purpose scheduler backed by `async_executor`.
- `FlowScheduler` shows deterministic block placement onto worker-local queues.

A minimal native scheduler usually needs:

- a clonable handle to an executor,
- worker thread lifecycle management,
- an implementation of `start_normal_domain()` that spawns every normal block and returns its tasks,
- an implementation of `spawn()` for unrelated async tasks.

## Selecting a Scheduler

Construct the runtime with your scheduler:

```rust
use futuresdr::prelude::*;

let scheduler = MyScheduler::new();
let rt = Runtime::with_scheduler(scheduler);

let fg = Flowgraph::new();
rt.run(fg)?;
```

Custom schedulers should preserve the runtime contract: every spawned block task must eventually return its block object, even if the block exits because the flowgraph was stopped. If a worker thread panics, treat it as a runtime failure rather than silently dropping block state.
