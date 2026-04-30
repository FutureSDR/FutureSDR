# Misc

Brief pointers to further information.

## Android

See [Android example](https://github.com/FutureSDR/FutureSDR/tree/main/examples/android/).

## WebAssembly

Use `trunk serve --release` to build and host the WebAssembly flowgraph. See the [ZigBee example](https://github.com/FutureSDR/FutureSDR/tree/main/examples/zigbee/).

## Web UI

FutureSDR's reusable web UI components are implemented in the [`prophecy`](https://github.com/FutureSDR/FutureSDR/tree/main/crates/prophecy) crate that is part of the FutureSDR repository. The default Prophecy GUI is served by the control port when a FutureSDR application is running, usually at `http://127.0.0.1:1337/`.

Prophecy is built with [Leptos](https://leptos.dev/), a Rust web framework for reactive user interfaces. It is intended both as a small default UI and as a component library for application-specific control panels.

The crate provides:

- `RuntimeHandle` and `FlowgraphHandle`: client-side handles for talking to the FutureSDR control-port API from the browser.
- `FlowgraphCanvas`: graphical flowgraph view with blocks, stream edges, message edges, and clickable message inputs.
- `FlowgraphTable`: table view of block IDs, instance names, stream ports, message ports, and blocking state.
- `Pmt`, `PmtInput`, `PmtInputList`, and `PmtEditor`: components for displaying, entering, and submitting PMT values.
- `RadioSelector`, `ListSelector`, and `Slider`: simple controls that post PMT values to a block message handler.
- `TimeSink`: WebGL time-domain display that reads samples from a websocket or a Leptos signal.
- `Waterfall`: WebGL waterfall display for streaming spectral data.
- `ConstellationSink` and `ConstellationSinkDensity`: WebGL constellation displays for complex sample streams.
- `ArrayView`: helper trait for exposing Rust numeric slices as JavaScript typed-array views for WebGL uploads.

For a custom web GUI that uses Prophecy components in an application-specific layout, see the [WLAN example](https://github.com/FutureSDR/FutureSDR/tree/main/examples/wlan).

