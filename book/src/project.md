# Project Creation

To create a Rust crate that uses FutureSDR initialize the crate and add FutureSDR as a dependency.

```bash
cargo init my_project
cd my_project
```

Edit the `Cargo.toml` to add the dependency. There are several options:

**Use a specific version** (stable, but code might be outdated due to irregular release cycles)
```toml
[dependencies]
futuresdr = { version = "0.0.39" }
```

**Track the main branch** (unstable but always up-to-date)
```toml
[dependencies]
futuresdr = { git = "https://github.com/FutureSDR/FutureSDR.git", branch = "main" }
```

**Use a specific commit** (potentially best of both worlds)
```toml
[dependencies]
futuresdr = { git = "https://github.com/FutureSDR/FutureSDR.git", rev = "7afd76c6d768ebc6432e705efe13e73543d33668" }
```

**Use a local working tree** (if you work on FutureSDR in parallel)
```toml
[dependencies]
futuresdr = { path = "../FutureSDR" }
```


## Features

FutureSDR supports several features that you may want to enable.

- `default`: by default `tracing_max_level_debug` and `tracing_release_max_level_info` are enabled
- `aaronia_http`: drivers for Aaronia HTTP servers, usable through Seify
- `audio`: read/write audio files and interface speakers/mic
- `burn`: buffers using [Burn](https://burn.dev) tensors
- `flow_scheduler`: enable the [Flow Scheduler](scheduler.md#flow)
- `hackrf`: enable Rust HackRF driver for Seify (unstable, not recommended)
- `rtlsdr`: enable Rust RTL SDR driver for Seify (unstable, not recommended)
- `seify`: enable Seify SDR hardware abstraction
- `seify_dummy`: enable dummy driver for Seify for use in unit tests
- `soapy`: enable SoapySDR driver for Seify
- `tracing_max_level_debug`: disable tracing messages in debug mode (compile-time filter)
- `tracing_release_max_level_info`: disable debug and tracing messages in release mode (compile-time filter)
- `vulkan`: enable Vulkan buffers and blocks
- `wgpu`: enable WGPU buffers and blocks
- `zeromq`: enable ZeroMQ source and sink
- `zynq`: enable Xilinx Zynq DMA buffers

For example:

```toml
[dependencies]
futuresdr = { version = "0.0.39", default-features = false, features = ["audio", "seify"] }
```


## Minimal Example

To test if everything is working, you can paste the following minimal example in `src/main.rs` and execute it with `cargo run`.

```rust
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::prelude::*;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = NullSource::<u8>::new();
    let head = Head::<u8>::new(123);
    let snk = NullSink::<u8>::new();

    connect!(fg, src > head > snk);

    Runtime::new().run(fg)?;

    Ok(())
}
```
