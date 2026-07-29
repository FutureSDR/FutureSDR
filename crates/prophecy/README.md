# Prophecy GUI for FutureSDR

This crate implements a GUI for active FutureSDR flowgraphs. The repository
keeps a pre-built bundle in `dist/`, which the FutureSDR control port discovers
when an application runs from the repository checkout.

The published `futuresdr` crate does not embed these assets. Applications built
outside the repository can build Prophecy themselves and set
`frontend_path` in the FutureSDR configuration to the resulting directory.

For examples of advanced configurations, see the following:

* [`wlan`](https://github.com/futuresdr/futuresdr/blob/main/examples/wlan/src/wasm/frontend.rs)
* [`spectrum`](https://github.com/futuresdr/futuresdr/blob/main/examples/spectrum/src/wasm/web.rs)
* [`zigbee`](https://github.com/futuresdr/futuresdr/blob/main/examples/zigbee/src/frontend.rs)

Note: _Prophecy_ is still under development, and is not yet fully functional nor API stable.

## Development

_Prophecy_ is implemented using [Leptos](https://leptos.dev), with building/bundling via [Trunk](https://trunkrs.dev).

### Pre-requisites
`trunk` may be installed with `cargo`:

    cargo install trunk

Other installation options are described [here](https://trunkrs.dev/#getting-started).

You will need the WebAssembly target installed:

    rustup target add wasm32-unknown-unknown

### Building

_Prophecy_ is built with:

    trunk build --release

The output is rendered to `dist/`.


