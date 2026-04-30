# Block

This page is for application developers who want to instantiate existing blocks and wire them into a [Flowgraph](flowgraph.md). It does not cover implementing custom blocks.

A block is a processing element with stream ports, message ports, or both. In application code, using a block usually means:

1. construct the block,
2. add it to a flowgraph, and
3. connect it to other blocks.

```rust
use futuresdr::blocks::Head;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;

let mut fg = Flowgraph::new();

let src = VectorSource::<f32>::new(vec![1.0, 2.0, 3.0, 4.0]);
let head = Head::<f32>::new(2);
let snk = VectorSink::<f32>::new(2);

connect!(fg, src > head > snk);

let fg = Runtime::new().run(fg)?;
let snk = fg.block(&snk)?;

assert_eq!(snk.items(), &vec![1.0, 2.0]);
```

## Type Parameters

Many blocks are generic over the sample type they process. For example, `VectorSource::<f32>` produces `f32` samples, while `VectorSource::<u8>` produces bytes.

Some blocks are also generic over their buffer implementation. This can make their full Rust type look large, but most application code should ignore the buffer type. In-tree blocks use default CPU buffers that are usually the right choice:

```rust
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;

let src = NullSource::<f32>::new();
let head = Head::<f32>::new(1024);
let snk = NullSink::<f32>::new();
```

Here, `Head::<f32>` is enough even though there are two more generic parameters to specify input and output buffer types. These types are filled in by the block's defaults.

## When Type Inference Needs Help

Rust can infer a block's sample type when constructor arguments carry enough type information. For example, the vector below contains `u32` values, so the source item type is known:

```rust
use futuresdr::blocks::VectorSource;

let src = VectorSource::new(vec![1_u32, 2, 3]);
```

Other constructors do not mention the sample type in their arguments. `Head::new(1024)` only says how many items to pass through; it does not say what item type the block should process. In those cases, provide the sample type explicitly:

```rust
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;

let head = Head::<f32>::new(1024);
let snk = NullSink::<f32>::new();
```

This is also why examples sometimes repeat the sample type even though the remaining generic parameters have defaults. Rust allows default generic parameters, but it cannot always infer the earlier parameters that those defaults depend on.

Closure-based blocks usually infer their types from the closure argument and return type. Add an argument annotation when needed:

```rust
use futuresdr::blocks::Apply;

let scale = Apply::new(|x: &f32| x * 0.5);
let to_u32 = Apply::new(|x: &f32| *x as u32);
```

## Builders

Some blocks have a simple `new(...)` constructor; others use a builder. Typically, a builder is available when there are optional parameters that clutter `new()` with many optional arguments.

```rust
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::prelude::*;
use std::time::Duration;

let src = MessageSourceBuilder::new(Pmt::U32(42), Duration::from_millis(100))
    .n_messages(10)
    .build();
```

## Inspecting Blocks After Run

When `Runtime::run()` returns, the finished flowgraph is returned too. Keep the `BlockRef` from `connect!` or `fg.add()` if you want to inspect block state afterwards:

```rust
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;

let mut fg = Flowgraph::new();

let src = VectorSource::<u8>::new(vec![1, 2, 3]);
let snk = VectorSink::<u8>::new(3);

connect!(fg, src > snk);

let fg = Runtime::new().run(fg)?;
let snk = fg.block(&snk)?;

assert_eq!(snk.items(), &vec![1, 2, 3]);
```

For running flowgraphs, use the `RunningFlowgraph` or `FlowgraphHandle` APIs described in the [Flowgraph](flowgraph.md#flowgraph-interactions) and [Runtime](runtime.md) chapters.
