# Custom Blocks

Custom blocks implement the processing logic that runs inside a flowgraph. A block usually consists of:

- stream input/output fields marked with `#[input]` and `#[output]`,
- optional message inputs and outputs declared on the struct,
- a `Kernel` implementation with `init()`, `work()`, and `deinit()` methods.

Blocks derive `Block` and implement `Kernel`. Blocks that need non-`Send` state, non-`Send` futures, or local-only buffers can run in a local domain.

## Stream Block

This block multiplies `f32` samples by a fixed gain:

```rust
use futuresdr::runtime::dev::prelude::*;

#[derive(Block)]
pub struct Scale {
    #[input]
    input: DefaultCpuReader<f32>,
    #[output]
    output: DefaultCpuWriter<f32>,
    gain: f32,
}

impl Scale {
    pub fn new(gain: f32) -> Self {
        Self {
            input: DefaultCpuReader::default(),
            output: DefaultCpuWriter::default(),
            gain,
        }
    }
}

impl Kernel for Scale {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let input = self.input.slice();
        let output = self.output.slice();
        let input_len = input.len();
        let n = input_len.min(output.len());

        for i in 0..n {
            output[i] = input[i] * self.gain;
        }

        self.input.consume(n);
        self.output.produce(n);

        if self.input.finished() && n == input_len {
            io.finished = true;
        }

        Ok(())
    }
}
```

`slice()` returns the currently available readable or writable window. After processing, call `consume(n)` and `produce(n)` with the number of items actually handled.

Set `io.call_again = true` when the block knows it can make more progress immediately. Set `io.finished = true` when the block is done and downstream ports should be notified.
An input can report `finished()` while its final items are still readable, so a
transform block must not finish until it has consumed the complete current input
window.

## Message Ports

Message inputs are declared with `#[message_inputs(...)]`. Each listed handler is an async method on the block. Message outputs are declared with `#[message_outputs(...)]` and used through `MessageOutputs`.

```rust
use futuresdr::runtime::dev::prelude::*;

#[derive(Block)]
#[message_inputs(set_gain)]
#[message_outputs(changed)]
pub struct AdjustableScale {
    #[input]
    input: DefaultCpuReader<f32>,
    #[output]
    output: DefaultCpuWriter<f32>,
    gain: f32,
}

impl AdjustableScale {
    async fn set_gain(
        &mut self,
        _io: &mut WorkIo,
        mo: &mut MessageOutputs,
        _meta: &BlockMeta,
        p: Pmt,
    ) -> Result<Pmt> {
        self.gain = f64::try_from(p)? as f32;
        mo.post("changed", Pmt::F32(self.gain)).await?;
        Ok(Pmt::Ok)
    }
}
```

Use `#[message_inputs(set_gain = "gain")]` when the public port name should differ from the Rust method name. This is useful for raw identifiers or compatibility with an existing control API.

## Lifecycle Methods

`Kernel` methods are called in this order:

- `init()`: once, after stream ports have been connected and validated.
- `work()`: repeatedly, whenever data, messages, timers, or explicit wakeups make progress possible.
- `deinit()`: once, during shutdown.

All methods receive `MessageOutputs` and read-only `BlockMeta`. `work()` also receives `WorkIo`, which is the block's way to communicate scheduling decisions back to the runtime.

Use `io.block_on()` when the block should sleep until the future returned by `Kernel::block_on()` completes. The block may still be called earlier if stream data or a message arrives.

## Local Blocks

Use a local domain for non-`Send` state or non-`Send` futures:

```rust
use futuresdr::runtime::dev::prelude::*;

#[derive(Block)]
pub struct UiBoundBlock {
    #[input]
    input: LocalCpuReader<f32>,
}

impl Kernel for UiBoundBlock {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        if self.input.finished() {
            io.finished = true;
        }
        Ok(())
    }
}
```

Add local blocks to a `LocalDomain` with `Flowgraph::with_local_domain()`. The closure is executed in the local domain, so non-`Send` resources can be created there:

```rust
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::LocalCpuReader;

let mut fg = Flowgraph::new();
let local = fg.local_domain()?;

let block = fg.with_local_domain(local, |ctx| {
    Ok(ctx.add(UiBoundBlock {
        input: LocalCpuReader::<f32>::default(),
    }))
})?;
```

For async construction on the local thread, use `Flowgraph::with_local_domain_async()` and add blocks through the provided `LocalDomainContext`. On native targets a local domain is backed by a dedicated thread; on WASM it is backed by a dedicated web worker.

## Testing

Use the [Mocker](mocker.md) to test one block without a full runtime. For graph-level behavior, build a small flowgraph with finite sources and sinks and run it with `Runtime::run()`.
