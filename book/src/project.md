# Project Creation

To create a Rust crate that uses FutureSDR, initialize the crate and add FutureSDR as a dependency. FutureSDR requires nightly Rust, so configure the project to use the nightly toolchain.

```bash
cargo init my_project
cd my_project
rustup override set nightly
```

Edit the `Cargo.toml` to add the dependency. There are several options:

**Use a specific version** (stable, but code might be outdated due to irregular release cycles)
```toml
[dependencies]
futuresdr = { version = "0.6.0" }
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

FutureSDR keeps common native application functionality in default features and
puts hardware drivers, optional integrations, and development-only helpers
behind explicit Cargo features. Disable default features only when you want a
smaller dependency graph or different compile-time logging filters, then enable
the pieces your application needs explicitly.

- `default`: enables `ctrl_port`, `tracing_max_level_debug`, and `tracing_release_max_level_info`
- `aaronia_http`: drivers for Aaronia HTTP servers, usable through Seify
- `audio`: read/write audio files and interface speakers/mic
- `burn`: buffers using [Burn](https://burn.dev) tensors
- `ctrl_port`: enable the native HTTP control port and static frontend server
- `flow_scheduler`: enable the [Flow Scheduler](scheduler.md#flow)
- `hackrf`: enable Rust HackRF driver for Seify (unstable, not recommended)
- `hydrasdr`: enable the native and WebUSB async HydraSDR driver for Seify
- `mocker`: enable the native-only [`Mocker`](mocker.md) test and benchmark harness
- `rtlsdr`: enable Rust RTL SDR driver for Seify (unstable, not recommended)
- `seify`: enable Seify SDR hardware abstraction
- `seify_dummy`: enable dummy driver for Seify for use in unit tests
- `soapy`: enable SoapySDR driver for Seify
- `tracing_max_level_debug`: compile out `trace` messages in debug mode
- `tracing_release_max_level_info`: compile out messages more detailed than `info` in release mode
- `wgpu`: enable WGPU buffers and blocks
- `zeromq`: enable ZeroMQ source and sink

For example:

```toml
[dependencies]
futuresdr = { version = "0.6.0", default-features = false, features = ["audio", "seify", "ctrl_port"] }
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
