FutureSDR Macros
================

Helpers library of macros to ease FutureSDR usage


## Flowgraph macro

Remove boilerplate when creating the flow graph.
One just have to express the connexions like this:

```rust
#[flowgraph(fg)]
{
    src > shift;
    shift > resamp1;
    resamp1 > demod;
    demod > resamp2;
    resamp2 > snk;
};
```

And it generates a code equivalent to:

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

When ports have different name than standard `in` and `out`, one can use following notation.

NB: `in` is a reserved rust keyword and thus must be escaped as `r#in`.

```rust
#[flowgraph(fg)]
{
    blk1.out2 > blk2.samples;
    blk2 > blk3.input;
    blk3.output > blk4.r#in;
};
```