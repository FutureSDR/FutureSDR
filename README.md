# FutureSDR

An experimental asynchronous SDR runtime for heterogeneous architectures that
is:

* **Extensible**: custom buffers (supporting accelerators like GPUs and FPGAs)
  and custom schedulers (optimized for your application).

* **Asynchronous**: solving long-standing issues around IO, blocking, and
  timers.

* **Portable**: Linux, Windows, Mac, WASM, Android, and prime support for
  embedded platforms through a REST API and web-based GUIs.

* **Fast**: SDR go brrr!

[![Crates.io][crates-badge]][crates-url]
[![Apache 2.0 licensed][apache-badge]][apache-url]
[![Build Status][actions-badge]][actions-url]

[crates-badge]: https://img.shields.io/crates/v/futuresdr.svg
[crates-url]: https://crates.io/crates/futuresdr
[apache-badge]: https://img.shields.io/badge/license-Apache%202-blue
[apache-url]: https://github.com/futuresdr/futuresdr/blob/master/LICENSE
[actions-badge]: https://github.com/futuresdr/futuresdr/workflows/CI/badge.svg
[actions-url]: https://github.com/futuresdr/futuresdr/actions?query=workflow%3ACI+branch%3Amaster

[Website](https://www.futuresdr.org) |
[Guides](https://www.futuresdr.org/tutorial) |
[API Docs](https://docs.rs/futuresdr/latest/futuresdr) |
[Chat](https://discord.com/invite/vCz29eDbGP/)

## Overview

FutureSDR supports *Blocks* with synchronous or asynchronous implementations for
stream-based or message-based data processing. Blocks can be combined to a
*Flowgraph* and launched on a *Runtime* that is driven by a *Scheduler*.

* Single and multi-threaded schedulers, including examples for
  application-specific implementations.
* Portable GPU acceleration using the Vulkan API (supports Linux, Windows,
  Android, ...).
* User space DMA driver for Xilinx Zynq to interface FPGAs.

## Development

Since FutureSDR is in an early state of development, it is likely that SDR
applications will require changes to the runtime. We, therefore, do not
recommend to add it as a dependency in a separate project but to clone the
repository and implement the application as binary, example, or sub-crate.

## Example

An example flowgraph that forwards 123 zeros into a sink:

``` rust
use futuresdr::anyhow::Result;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::NullSource;
use futuresdr::macros::connect;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;

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

## Contributing

Contributions are very welcome. Please see the (work-in-progress) [contributing
guide][contr] for more information. If you develop larger features or work on
major changes with the main intention to submit them upstream, it would be
great, if you could announce them in advance.

[contr]: https://github.com/futuresdr/futuresdr/blob/master/CONTRIBUTING.md

## Conduct

The FutureSDR project adheres to the [Rust Code of Conduct][coc]. It describes
the _minimum_ behavior expected from all contributors.

[coc]: https://github.com/rust-lang/rust/blob/master/CODE_OF_CONDUCT.md

## License

This project is licensed under the [Apache 2.0 license][lic].

Using this license is in contrast to the large majority of Open Source SDR
applications and frameworks, which are mostly AGLP, LGPL, or GPL. In a nutshell,
this means that there is *no* money to be made from relicensing the project for
commercial use, since this is already allowed by Apache 2.0. Furthermore,
companies can use (parts of) the project and integrate (adapted) versions in
commercial products without releasing the source or contributing back to the
project.

The main motivation for this license is that
* it better fits the Rust ecosystem
* it eases adoption; one can use (parts of) the code with close to no strings
  attached
* using Open Source and not contributing back (for the time being) seems better
  than not using Open Source at all

[lic]: https://github.com/futuresdr/futuresdr/blob/master/LICENSE.

## Contributions

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in FutureSDR, shall be licensed as Apache 2.0, without any
additional terms or conditions.
