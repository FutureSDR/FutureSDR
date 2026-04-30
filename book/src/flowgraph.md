# Flowgraph

A [`Flowgraph`](https://docs.rs/futuresdr/latest/futuresdr/runtime/struct.Flowgraph.html) is a directed graph of blocks and connections. Blocks do the actual work; the flowgraph describes which stream ports and message ports are connected.

Stream connections carry sample streams between blocks. They must form a directed acyclic graph. Message connections carry PMTs between message handlers and can use arbitrary topologies.

## Constructing Flowgraphs

Create an empty flowgraph with `Flowgraph::new()`, add blocks, and connect them. The usual way to build a flowgraph is the `connect!` macro. It adds blocks to the flowgraph if needed and wires their ports.

The simplest stream connection uses the default stream output and input port names:

```rust
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;

let mut fg = Flowgraph::new();

let src = NullSource::<u8>::new();
let head = Head::<u8>::new(1024);
let snk = NullSink::<u8>::new();

connect!(fg, src > head > snk);
```

Named stream ports can be selected explicitly. Output ports are written after the source block, and input ports are written before the destination block:

```rust
connect!(fg, src.output > input.head > snk);
```

Message connections use `|` instead of `>`. This example connects the `out` message output of `msg_source` to the `in` message input of `msg_copy`, then forwards messages to `msg_sink`:

```rust
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::prelude::*;
use std::time::Duration;

let mut fg = Flowgraph::new();

let msg_source = MessageSourceBuilder::new(Pmt::String("foo".to_string()), Duration::from_millis(100))
    .n_messages(20)
    .build();
let msg_copy = MessageCopy::new();
let msg_sink = MessageSink::new();

connect!(fg, msg_source | msg_copy | msg_sink);
```

Message ports can also be named explicitly:

```rust
connect!(fg, msg_source.out | r#in.msg_copy);
```

The `r#in` spelling is Rust's raw-identifier syntax for a port named `in`.

Stream and message connections can be mixed in one macro invocation. Separate independent connections with semicolons:

```rust
connect!(fg,
    src > head > snk;
    msg_source | msg_copy | msg_sink;
);
```

Blocks can also be added and connected manually. This is what the macro is doing for the common cases: it stores blocks in the flowgraph, gets their port endpoints, and records stream or message edges.

```rust
use futuresdr::blocks::MessageCopy;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::MessageSourceBuilder;
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use std::time::Duration;

let mut fg = Flowgraph::new();

let src = fg.add(VectorSource::<u32>::new(vec![1, 2, 3, 4]));
let snk = fg.add(VectorSink::<u32>::new(4));
fg.stream_dyn(src, "output", snk, "input")?;

let msg_source = fg.add(
    MessageSourceBuilder::new(Pmt::String("foo".to_string()), Duration::from_millis(100))
        .n_messages(20)
        .build(),
);
let msg_copy = fg.add(MessageCopy::new());
let msg_sink = fg.add(MessageSink::new());

fg.message(msg_source, "out", msg_copy, "in")?;
fg.message(msg_copy, "out", msg_sink, "in")?;

let fg = Runtime::new().run(fg)?;
```

Use `connect!` for normal application code. The explicit form is useful when block types are selected dynamically or when it helps to understand the lower-level API.

## Accessing Blocks

When a block is added to a flowgraph, FutureSDR returns a `BlockRef<T>`. A block reference is a lightweight typed identifier. It is copyable, can be converted to a `BlockId`, and can be used to access the block while the flowgraph owns it.

The `connect!` macro also leaves you with block references for the blocks it added. After a blocking `Runtime::run()`, the finished `Flowgraph` is returned, so the same `BlockRef` can be used to inspect block state:

```rust
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;

let mut fg = Flowgraph::new();

let src = VectorSource::<u32>::new(vec![1, 2, 3, 4]);
let snk = VectorSink::<u32>::new(4);

connect!(fg, src > snk);

let fg = Runtime::new().run(fg)?;
let snk = fg.block(&snk)?;

assert_eq!(snk.items(), &vec![1, 2, 3, 4]);
```

Similarly, `block_mut()` can be used to update block metadata or block state:

```rust
let mut fg = Flowgraph::new();
let snk = fg.add(VectorSink::<u32>::new(4));

fg.block_mut(&snk)?.set_instance_name("samples");
```

Use `BlockRef::id()` or convert a `BlockRef` into `BlockId` when a runtime interaction API needs an untyped block identifier.

## Flowgraph Interactions

`Runtime::run()` is the simplest way to execute a flowgraph when you only need the result after it finishes. To interact with a flowgraph while it is running, start it with `Runtime::start()` on native targets or `Runtime::start_async()` in async code. Both return a [`RunningFlowgraph`](https://docs.rs/futuresdr/latest/futuresdr/runtime/struct.RunningFlowgraph.html).

`RunningFlowgraph` can post messages, call message handlers, describe the running graph, stop it, and wait for completion.

The following example starts a flowgraph and continuously hops through a list of frequencies by posting `Pmt::F64` values to a block's `freq` message handler:

```rust
use futuresdr::prelude::*;
use std::time::Duration;

let mut fg = Flowgraph::new();
// set up the flowgraph

// `my_seify_source` is a source or sink block with a `freq` message input.
let radio = fg.add(my_seify_source);
let radio_id: BlockId = radio.into();

let rt = Runtime::new();
let running = rt.start(fg)?;

Runtime::block_on(async move {
    let frequencies = [100.0e6, 101.0e6, 102.0e6];

    loop {
        for freq in frequencies {
            running.post(radio_id, "freq", Pmt::F64(freq)).await?;
            Timer::after(Duration::from_secs(1)).await;
        }
    }
})?;
```

Waiting for completion is a separate operation. Use it when the flowgraph is expected to finish on its own, for example when a finite source reaches the end of its input:

```rust
use futuresdr::blocks::VectorSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;

let mut fg = Flowgraph::new();

let src = VectorSource::<u32>::new(vec![1, 2, 3, 4]);
let snk = VectorSink::<u32>::new(4);

connect!(fg, src > snk);

let rt = Runtime::new();
let running = rt.start(fg)?;

let fg = running.wait()?;
let snk = fg.block(&snk)?;

assert_eq!(snk.items(), &vec![1, 2, 3, 4]);
```

For flowgraphs that do not finish on their own, request shutdown before waiting:

```rust
Runtime::block_on(async move {
    running.stop().await?;
    let fg = running.wait_async().await?;
    Ok::<_, Error>(fg)
})?;
```

If multiple tasks need access to the same running flowgraph, keep a clonable handle:

```rust
let handle = running.handle();
Runtime::block_on(async move {
    handle.post(radio_id, "freq", Pmt::F64(100.0e6)).await
})?;
```
