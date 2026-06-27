FutureSDR Macros
================

Procedural macros for FutureSDR applications and custom blocks.

## `connect!`

`connect!` adds blocks to a flowgraph or local-domain context if needed and
wires stream, local-stream, and message connections.

```rust
connect!(fg,
    src > head > snk;
    msg_source | msg_sink;
);
```

Default stream ports are `output` and `input`. Default message ports are `out`
and `in`. Named ports can be written on either side of a connection:

```rust
connect!(fg,
    src.samples > input.filter.output > snk;
    control.out | command.radio;
);
```

Connection operators:

- `>`: send-capable stream connection.
- `~>`: local-domain-only stream connection for non-`Send` buffers, used with a `LocalDomainContext`.
- `|`: message connection.

Reusable in-place buffers recycle automatically when the final owner drops the
buffer, so there is no separate circuit-closing operator.

Blocks without connections can be listed on their own line:

```rust
connect!(fg, monitor);
```

## `#[derive(Block)]`

Derive the runtime interface for a block kernel. Annotated fields become
stream ports, and struct-level attributes declare message ports:

```rust
#[derive(Block)]
#[message_inputs(set_gain)]
#[message_outputs(done)]
struct Scale {
    #[input]
    input: DefaultCpuReader<f32>,
    #[output]
    output: DefaultCpuWriter<f32>,
    gain: f32,
}
```

The struct implements `Kernel`; the derive macro supplies the generated port
metadata, stream initialization and validation, dynamic stream lookup, and
message-handler dispatch.

Supported attributes:

- `#[input]` and `#[output]` on fields.
- `#[message_inputs(handler)]`.
- `#[message_inputs(handler = "port-name")]`.
- `#[message_outputs(out)]`.
- `#[blocking]`.
- `#[type_name(Name)]`.
- `#[null_kernel]`.
