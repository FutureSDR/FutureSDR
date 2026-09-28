# ZigBee WASM Receiver

Browser-based ZigBee receiver using Seify's async WebUSB backend with bladeRF 1, HackRF, or UHD (USRP B2xx). The signal-processing blocks live in `../zigbee`; this crate only contains the WASM UI and flowgraph wiring.

The `bladerf1` feature enables Seify's native Rust bladeRF 1 backend and requires Rust 1.98.1 or newer.

Build and serve with Trunk. The `.cargo/config.toml` enables the WASM atomics/shared-memory settings and `Trunk.toml` serves the required COOP/COEP headers:

```sh
trunk serve
```

Then open <http://127.0.0.1:8080/>, click **Start RX**, and select the radio in the browser's WebUSB permission chooser. The gain slider uses the selected radio's supported range.

A USRP may reconnect after loading firmware. If prompted to request permission again, click **Start RX** again and select the reconnected device. FPGA loading can take several seconds.
