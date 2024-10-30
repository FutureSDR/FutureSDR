# Prophecy GUI for FutureSDR

This crate implements the GUI served by [`futuresdr::runtime::ControlPort`](../../src/runtime/ctrl_port.rs) on active `FlowGraphs`.
It gets bundled automatically with the `futuresdr` distribution.

By default, _Prophecy_ available at `http://localhost:1337/` when running a `futuresdr` application.

For examples of advanced configurations, see the following:

* [`wlan`](../../examples/wlan/src/wasm/frontend.rs) 
* [`spectrum`](../../examples/spectrum/src/wasm/web.rs)
* [`zigbee`](../../examples/zigbee/src/frontend.rs)

Note: _Prophecy_ it is still under development, and is not yet fully functional nor API stable.

## Development

_Prophecy_ is implemented using [Leptos](https://leptos.dev), with building/bundling via [Trunk](https://trunkrs.dev).

### Pre-requisites
`trunk` may be installed with `cargo`:

    cargo install trunk

Other installation options described [here](https://trunkrs.dev/#getting-started).

You will need the WebAssembly target installed:

    rustup target add wasm32-unknown-unknown

### Building

_Prophecy_ is built with:

    trunk build --release

The output is rendered to `dist/`.




