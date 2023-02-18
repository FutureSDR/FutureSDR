FutureSDR Macros
================

Macros to make working with FutureSDR a bit nicer.

## Connect Macro

Avoid boilerplate when setting up the flowgraph. This macro simplifies adding
blocks to the flowgraph and connecting them.

Assume you have created a flowgraph `fg` and several blocks (`src`, `shift`, ...) and need to add the block to the flowgraph and connect them. Using the `connect!` macro, this can be done with:

```rust
connect!(fg,
    src.out > shift.in;
    shift > resamp1 > demod;
    demod > resamp2 > snk;
);
```

It generates the following code:

```rust
// Add all the blocks to the `Flowgraph`...
let src = fg.add_block(src);
let shift = fg.add_block(shift);
let resamp1 = fg.add_block(resamp1);
let demod = fg.add_block(demod);
let resamp2 = fg.add_block(resamp2);
let snk = fg.add_block(snk);

// ... and connect the ports appropriately
fg.connect_stream(src, "out", shift, "in")?;
fg.connect_stream(shift, "out", resamp1, "in")?;
fg.connect_stream(resamp1, "out", demod, "in")?;
fg.connect_stream(demod, "out", resamp2, "in")?;
fg.connect_stream(resamp2, "out", snk, "in")?;
```

Connections endpoints are defined by `block.port_name`. Standard names (i.e.,
`out`/`in`) can be omitted. When ports have different name than standard `in`
and `out`, one can use following notation.

Stream connections are indicated as `>`, while message connections are indicated as `|`.

It is possible to add blocks that have no connections by just putting them on a line separately.

``` rust
connect!(fg, dummy);
```

Port names with spaces have to be quoted.

```ignore
connect!(fg,
    src."out port" > snk
);
```

## Message Handler Macro

Avoid boilerplate when creating message handlers.

Assume a block with a message handler that refers to a block function
`Self::my_handler`.

```ignore
pub fn new() -> Block {
    Block::new(
        BlockMetaBuilder::new("MyBlock").build(),
        StreamIoBuilder::new().build(),
        MessageIoBuilder::new()
            .add_input("handler", Self::my_handler)
            .build(),
        Self,
    )
}
```

The underlying machinery of the handler implementation is rather involved.
With the `message_handler` macro, it can be simplified to:

```ignore
#[message_handler]
async fn my_handler(
    &mut self,
    _mio: &mut MessageIo<Self>,
    _meta: &mut BlockMeta,
    _p: Pmt,
) -> Result<Pmt> {
    Ok(Pmt::Null)
}
```
