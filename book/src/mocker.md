# Mocker

The `Mocker` is a small harness for running one block directly, without building a full `Flowgraph` and without starting a `Runtime`.

This is useful for:

- unit tests for a single block,
- checking edge cases with carefully chosen input samples,
- testing message handlers and message outputs,
- microbenchmarks where the scheduler and graph setup would hide the cost of the block itself.

`Mocker` is available on native targets through `futuresdr::runtime::mocker`.

## Stream Blocks

For stream blocks, instantiate the block with `mocker::Reader<T>` and `mocker::Writer<T>` buffer types. Then set the input samples, reserve output space, and run the block:

```rust
use futuresdr::blocks::Apply;
use futuresdr::runtime::mocker::Mocker;
use futuresdr::runtime::mocker::Reader;
use futuresdr::runtime::mocker::Writer;

let block: Apply<_, _, _, Reader<u32>, Writer<u32>> =
    Apply::with_buffers(|x: &u32| x + 1);

let mut mocker = Mocker::new(block);
mocker.input().set(vec![1, 2, 3]);
mocker.output().reserve(3);

mocker.run();

let (items, tags) = mocker.output().get();

assert_eq!(items, vec![2, 3, 4]);
assert!(tags.is_empty());
```

The mock reader exposes the input through the same `CpuBufferReader` API that a normal block sees at runtime. The mock writer stores produced samples in a vector that can be read with `get()` or drained with `take()`.

## Multiple Runs

`run()` calls the block's `work()` method until the block stops requesting immediate re-entry through `WorkIo::call_again`. You can update the mocked input and run the same block again:

```rust
use futuresdr::blocks::Apply;
use futuresdr::runtime::mocker::Mocker;
use futuresdr::runtime::mocker::Reader;
use futuresdr::runtime::mocker::Writer;

let block: Apply<_, _, _, Reader<u32>, Writer<u32>> =
    Apply::with_buffers(|x: &u32| x + 1);

let mut mocker = Mocker::new(block);
mocker.output().reserve(6);

mocker.input().set(vec![1, 2, 3]);
mocker.run();

mocker.input().set(vec![4, 5, 6]);
mocker.run();

let (items, _) = mocker.output().get();

assert_eq!(items, vec![2, 3, 4, 5, 6, 7]);
```

If the block relies on `init()` or `deinit()` state, call those explicitly:

```rust
mocker.init();
mocker.run();
mocker.deinit();
```

## Tags

Mock inputs can include item tags:

```rust
use futuresdr::runtime::dev::ItemTag;
use futuresdr::runtime::dev::Tag;

mocker.input().set_with_tags(
    vec![0.0_f32; 1024],
    vec![ItemTag {
        index: 256,
        tag: Tag::Id(256),
    }],
);
```

The output writer returns both produced samples and output tags:

```rust
let (items, tags) = mocker.output().get();
```

## Message Blocks

`Mocker` can also exercise message handlers. Use `post()` to call a message handler on the wrapped block:

```rust
use futuresdr::blocks::MessageCopy;
use futuresdr::prelude::*;
use futuresdr::runtime::mocker::Mocker;

let mut mocker = Mocker::new(MessageCopy);

mocker.init();

let ret = mocker.post("in", Pmt::Usize(123))?;
assert_eq!(ret, Pmt::Ok);

mocker.run();

let messages = mocker.take_messages();
assert_eq!(messages, vec![vec![Pmt::Usize(123)]]);
```

Message outputs are captured per output port. Use `messages()` to clone the currently captured PMTs, or `take_messages()` to drain them.

## Benchmarks

Because `Mocker` runs a block without a scheduler, it is useful for measuring the cost of one block implementation. The repository's [apply benchmarks](https://github.com/FutureSDR/FutureSDR/blob/main/benches/apply.rs) use `Mocker` to compare several ways to apply a simple operation to samples.

For benchmark code that needs to call `Kernel::work()` directly, `parts_mut()` returns mutable access to the wrapped kernel, `MessageOutputs`, and `BlockMeta`:

```rust
let (kernel, message_outputs, meta) = mocker.parts_mut();
```

Most tests should prefer `mocker.run()`, since it matches the normal block work loop more closely.
