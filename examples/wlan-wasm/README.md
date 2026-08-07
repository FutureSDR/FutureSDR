# Wi-Fi WASM Receiver

This crate is the browser/WASM WLAN receiver. It uses the asynchronous Seify
WebUSB source with the HackRF backend, WASM scheduler workers, and the WLAN PHY
blocks in this example crate.

## Running

The receiver uses web workers and shared WASM memory, so build it with the
nightly toolchain configuration in `.cargo/config.toml` and serve it with the
COOP/COEP headers from `Trunk.toml`:

```sh
trunk serve
```

Then open <http://127.0.0.1:8080/> and grant WebUSB permission for the HackRF.

For the native loopback, RX, TX, and Prophecy-based constellation GUI, use the
separate `examples/wlan` crate.
