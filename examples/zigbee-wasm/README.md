# ZigBee WASM Receiver

Browser-based ZigBee receiver using Seify's async HackRF/WebUSB backend. The signal-processing blocks live in `../zigbee`; this crate only contains the WASM UI and flowgraph wiring.

Build and serve with Trunk. The `.cargo/config.toml` enables the WASM atomics/shared-memory settings and `Trunk.toml` serves the required COOP/COEP headers:

```sh
trunk serve
```

Then open <http://127.0.0.1:8080/> and grant WebUSB permission for the HackRF.
