# Custom Schedulers

Schedulers execute normal flowgraph block tasks and async tasks spawned through the runtime. Most applications should use `Runtime::new()` and the default `SmolScheduler`; write a scheduler only when you are experimenting with placement, latency, or executor integration.

The scheduler trait is:

```rust
pub trait Scheduler: Clone + Send + 'static {
    fn start_normal_domain(&self, spec: NormalDomainSpec) -> Result<NormalRunningDomain>;
    fn start_local_domain(&self, spec: LocalDomainSpec) -> Result<LocalRunningDomain>;

    fn spawn<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> Task<T>;
}
```

`start_normal_domain()` receives the normal send-capable blocks and domain topology. It usually spawns each block, calls `block.run(main_channel).await`, and returns task handles through `NormalRunningDomain`. The runtime waits for those tasks and restores the finished block objects into the returned flowgraph.

`start_local_domain()` receives a handle to an already-created local domain plus the local block slots assigned to it. The scheduler activates that existing domain; it does not create the local-domain thread or worker itself.

`spawn()` runs general async tasks on the scheduler. `Runtime::spawn()`, `Runtime::spawn_background()`, and control-plane internals use this method.

## Normal vs Local Work

Schedulers manage scheduling domains. The implicit normal domain contains send-capable block tasks. Local domains are created by the flowgraph for:

- blocks added through `Flowgraph::add_local()`,
- blocks marked with `#[blocking]`.

Blocking or thread-affine work should be placed in a local domain instead of being hidden inside the normal scheduler.

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
