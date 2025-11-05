# Project Creation

To create a Rust crates that uses FutureSDR initialize the crate and add FutureSDR as a dependency.

```bash
cargo init my_project
cd my_project
```

Edit the `Cargo.toml` to add the dependency. There are several options

**Use a specific version** (stable but code might be outdated due to unregular release cycles)
```toml
[dependencies]
futuresdr = { version = "0.0.39" }
```

**Track the main branch** (unstable but always up-to-date)
```toml
[dependencies]
futuresdr = { git = "https://github.com/futuresdr/futuresdr.git", branch = "main" }
```

**Use a specific commit** (potentially best of both worlds)
```toml
[dependencies]
futuresdr = { git = "https://github.com/futuresdr/futuresdr.git", rev = "7afd76c6d768ebc6432e705efe13e73543d33668" }
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
- `tracing_max_level_debug`:
- `tracing_release_max_level_info`:
- `vulkan`: enable Vulkan buffers and blocks
- `wgpu`:
- `zeromq`:
- `zynq`:

## Minimal Example

To check whether
