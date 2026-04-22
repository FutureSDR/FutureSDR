# Stream Port Design

FutureSDR stream ports currently live as typed fields on blocks. For example, a
CPU processing block usually has an input field implementing `CpuBufferReader`
and an output field implementing `CpuBufferWriter`. The fields are default
constructed when the block is constructed, initialized when the block is added
to a flowgraph, and connected when stream edges are created.

This design has a visible downside: buffer implementations have to represent an
unconnected state even though the real endpoint only exists after connection.
The circular buffer writer, for example, carries default block metadata and an
optional underlying writer until it is connected. This is not aesthetically
pleasing, and it mixes several responsibilities in one type:

- user-facing work-time API, such as `slice()`, `consume()`, and `produce()`;
- connect-time state, such as peer inboxes and port IDs;
- runtime lifecycle state, such as finish notifications;
- concrete buffer implementation state.

The obvious question is whether ports can be removed from block state and made
properties of flowgraph edges instead. That would match the conceptual model:
buffers transport data between blocks, so they look like edge state rather than
block state.

## Requirements

Any replacement design has to preserve several properties that are important for
FutureSDR.

- The hot `work()` path must remain statically dispatched. Blocks should call
  methods such as `slice()` or `get_buffer()` on concrete types without a
  `dyn` call on every access.
- External buffer implementations must remain possible. A design must not rely
  on a closed enum of built-in buffer types.
- Buffer implementations can expose very different work-time APIs. CPU buffers
  expose slices, in-place buffers exchange full and empty buffers, and GPU
  buffers expose backend-specific operations.
- Block work code must remain ergonomic. Wrapping every port in `Option` and
  unwrapping in `work()` is not acceptable.
- Flowgraph construction should remain ordinary Rust code. Users should be able
  to add blocks and connect them incrementally, without encoding the whole graph
  shape in type-level state.
- Typed stream connections should continue to catch incompatible buffer pairs at
  compile time where possible.
- Runtime introspection still needs stable block IDs, port IDs, and stream edge
  metadata.

These requirements constrain the design strongly. In Rust, avoiding dynamic
dispatch in `work()` means that the concrete stream endpoint type must be known
in the type that implements the block's runtime behavior. Once a block has been
added to the flowgraph and stored behind `dyn Block`, its concrete type cannot
be changed later by a connection.

## Alternatives Considered

### Runtime-owned typed streams

One idea is to remove stream ports from user block structs and pass a generated
stream bundle into `work()`:

```rust
async fn work(
    &mut self,
    io: &mut WorkIo,
    streams: &mut FilterStreams<I, O>,
    mo: &mut MessageOutputs,
    meta: &mut BlockMeta,
) -> Result<()>;
```

This can keep static dispatch if `FilterStreams<I, O>` contains the actual
reader and writer types. However, those types have to be known before the block
is inserted into the flowgraph as `dyn Block`. That either moves buffer choice
to block-add time or requires a staging builder that computes all port types
before materializing the runtime flowgraph.

Moving buffer choice to block-add time makes the API worse than the current
design, because the user still has to specify port types but through a second
mechanism. A type-level staging builder can preserve edge-owned buffer choice,
but it makes ordinary dynamic graph construction impractical and would likely
produce difficult compiler errors.

### Edge-owned buffers with capability objects

Another idea is for blocks to declare only stream capabilities, such as
`CpuInput<T>`, `CpuOutput<T>`, or `WgpuInput<T>`, while connections choose the
actual transport. This matches the conceptual model well.

The problem is the hot path. If a CPU input slot can hold any external CPU
reader implementation selected at connection time, then the slot needs either
dynamic dispatch, an enum of supported implementations, or type-level graph
staging. Dynamic dispatch is undesirable in `work()`, a closed enum rejects
external buffers, and type-level staging is too heavy for normal flowgraph
construction.

### `MaybeUninit`

Using `MaybeUninit` instead of `Option` does not solve the design issue. The
port still has an unconnected state, and the implementation still needs an
initialization flag, validation, correct drop handling, and a failure mode for
accidental use before connection. It mainly replaces safe code with unsafe code.

## Consequence

Given these constraints, typed stream ports need to remain part of the block
type. This is what lets Rust monomorphize `work()` for concrete buffer APIs
while still allowing external buffer implementations and incremental flowgraph
construction.

The current approach is therefore the practical baseline:

```rust
#[derive(Block)]
pub struct Filter<A, B, I = DefaultCpuReader<A>, O = DefaultCpuWriter<B>>
where
    I: CpuBufferReader<Item = A>,
    O: CpuBufferWriter<Item = B>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    f: Box<dyn FnMut(&A) -> Option<B> + Send + 'static>,
}
```

The concrete port fields make the block type reflect the exact work-time API.
This is important for buffers whose APIs are not slice based.

## Possible Cleanup Within the Current Approach

The remaining design problem is not that typed ports exist on blocks. The
problem is that every concrete buffer implementation currently has to implement
the unconnected port lifecycle itself.

A smaller, compatible cleanup is to split generic port lifecycle state from
connected buffer endpoint state:

```rust
pub struct PortReader<B: BufferFamily> {
    meta: PortMeta,
    config: B::ReaderConfig,
    endpoint: Late<B::Reader>,
}

pub struct PortWriter<B: BufferFamily> {
    meta: PortMeta,
    config: B::WriterConfig,
    endpoint: Late<B::Writer>,
    readers: Vec<Downstream>,
}
```

Concrete buffers would implement a family or endpoint trait. For example, the
circular buffer would provide connected reader and writer endpoints, while the
generic `PortReader` and `PortWriter` would own the unconnected state, block
metadata, validation, and common lifecycle behavior.

The public buffer names could stay the same through aliases or thin wrappers:

```rust
pub type DefaultCpuReader<T> = circular::Reader<T>;
pub type DefaultCpuWriter<T> = circular::Writer<T>;
```

where `circular::Reader<T>` and `circular::Writer<T>` are implemented in terms
of the generic port shell.

This keeps the important properties of the current design:

- block definitions remain typed and ergonomic;
- `work()` stays statically dispatched;
- external buffers remain possible;
- `connect!` can continue to operate on typed port fields;
- the unconnected `Option` state is centralized instead of repeated in every
  buffer implementation.

This does not make buffers purely edge-owned, but it addresses the main source
of ugliness without giving up the performance and extensibility properties that
the current design provides.
