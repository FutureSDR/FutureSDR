# Custom Buffers

Custom buffers define how stream samples move between blocks. A buffer implementation provides a writer type for the upstream block and a reader type for the downstream block. The basic connection contract is:

- the writer implements `BufferWriter`,
- the reader implements `BufferReader`,
- `BufferWriter::Reader` names the matching reader type,
- `connect()` stores the peer endpoints and any shared queues or resources,
- `validate()` fails if the port was never connected or is otherwise unusable.

Most custom buffers also implement one of the higher-level buffer families:

- `CpuBufferWriter` / `CpuBufferReader` for slice-based CPU buffers,
- `InplaceWriter` / `InplaceReader` / `InplaceBuffer` for reusable in-place chunks,
- accelerator-specific APIs for GPU, tensor, or DMA resources.

## Port State

Use `PortCore` and `ConnectionState` for the common lifecycle:

```rust
use futuresdr::runtime::buffer::*;
use futuresdr::runtime::dev::BlockInbox;
use futuresdr::runtime::{BlockId, Error, PortId};

pub struct MyWriter<T> {
    core: PortCore,
    peer: ConnectionState<PortEndpoint>,
    _type: std::marker::PhantomData<T>,
}

impl<T> Default for MyWriter<T> {
    fn default() -> Self {
        Self {
            core: PortCore::new_disconnected(),
            peer: ConnectionState::disconnected(),
            _type: std::marker::PhantomData,
        }
    }
}
```

`init()` binds a port to its owning block id, port id, and inbox. The derive macro calls it during flowgraph construction. `connect()` receives the matching peer during `Flowgraph::stream()` or the `connect!` macro expansion.

## Send-Capable Buffers

Normal native flowgraphs require send-capable buffers. The marker traits are implemented automatically when the concrete type and relevant futures satisfy the bounds:

- `SendBufferReader`
- `SendBufferWriter`
- `SendCpuBufferReader`
- `SendCpuBufferWriter`
- `SendInplaceReader`
- `SendInplaceWriter`

If a buffer is not `Send`, use a local domain and connect it with `stream_local()` or the `~>` operator in `connect!`.

## CPU Buffer Expectations

For CPU buffers, `slice_with_tags()` returns the currently readable or writable window. Tags use indices relative to that window. A block calls:

- `consume(n)` after reading `n` input items,
- `produce(n)` after writing `n` output items,
- `set_min_items(n)` to request a minimum number of items before work,
- `set_min_buffer_size_in_items(n)` to request a minimum backing capacity.

Do not advance read or write positions from `slice()` alone. The block controls advancement with `consume()` and `produce()`.

## Finish Notifications

Buffers propagate shutdown across stream edges:

- a reader's `notify_finished()` tells upstream writers that the downstream block is done,
- a writer's `notify_finished()` tells downstream readers that the upstream block is done,
- `finish()` marks the local side as finished,
- `finished()` lets the block observe that the peer is done.

Implementations normally use the peer `BlockInbox` or `BlockNotifier` stored during connection setup to wake the adjacent block.

## In-Place Circuits

In-place buffers move owned chunks through the graph. They are connected like normal streams:

```rust
connect!(fg, src > apply > snk);
```

A buffer carries its origin return handle while it is in flight. When the final owner drops the buffer, it automatically returns to the source writer's empty-buffer queue and wakes that writer. There is no separate circuit-closing connection or `<` operator.

## Validation and Testing

Validation should catch unconnected ports, wrong peer state, and missing hardware resources before any block `init()` method runs. Prefer returning `Error::ValidationError` with the owning block and port context when possible.

Use `Mocker` for CPU buffers used by one block. For a new buffer implementation, also add a small flowgraph test that connects a finite source to a finite sink and checks that finish notification, tags, and repeated scheduler calls behave correctly.
