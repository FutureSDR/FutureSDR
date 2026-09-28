# Wi-Fi WASM Receiver

This crate is the browser/WASM WLAN receiver. It uses the asynchronous Seify
WebUSB source with bladeRF 1, PlutoSDR, HackRF, or UHD (USRP B2xx), WASM scheduler
workers, and the WLAN PHY blocks in this example crate.

## Running

The receiver uses web workers and shared WASM memory, so build it with the
nightly toolchain configuration in `.cargo/config.toml` and serve it with the
COOP/COEP headers from `Trunk.toml`:

```sh
trunk serve
```

Then open <http://127.0.0.1:8080/> and click **Start RX**. Seify uses an already
authorized, accessible device, or opens the browser's WebUSB chooser with USB
filters for all enabled Seify backends. Select a supported device there. Seify
automatically chooses the backend when the RX worker opens the device. Channel
and gain can be changed live. RX DC correction is automatic: hardware correction
is enabled when supported, with a software filter otherwise. There is no DC toggle.

The example uses Seify 0.25.0 and the released Pluto driver from crates.io.
The `bladerf1` feature enables its native Rust bladeRF 1 backend and requires
Rust 1.98.1 or newer.
The `uhd` feature enables Seify's native Rust UHD backend. The `pluto` feature enables
its native IIO-over-USB driver.
Permission and worker-side opening use empty Seify arguments, with no preferred
driver. Pluto-specific RX settings are applied after Seify identifies the device.

The UHD backend supports B200, B210, B200mini, and B205mini reception. The UHD driver
automatically selects its 20 MHz clock mode for this receiver. HackRF also
supports the required 20 MS/s sample rate.

For the native loopback, RX, TX, and Prophecy-based constellation GUI, use the
separate `examples/wlan` crate.

## PlutoSDR

Pluto uses its native IIO USB endpoints through Seify and nusb. It is configured
for 20 MS/s, 20 MHz RX bandwidth, the A_BALANCED input (the RX connector), and
50 dB manual gain. The WLAN decoder still requires the 20 MHz sample clock;
substituting a lower sample rate would not decode ordinary 20 MHz WLAN frames.
The gain slider and supported WLAN channels use firmware-reported ranges, so
the stock AD9363 tuning range does not offer unsupported 5 GHz WLAN
channels once the source is open. Gain and channel changes pause/restart RX.

**Pluto reception is experimental at this rate.** Its current IIOD stream uses
four bytes per complex sample: 20 MS/s requires 80 MB/s, exceeding the USB 2.0
link's nominal 60 MB/s before overhead. Samples/frames can be lost even when
the sample-clock readback is 20 MHz. The source logs delivered throughput every
five seconds' worth of samples; the legacy stream has no reliable loss counter.
Continuous reception needs a more compact device-side sample format. Host-side
conversion or lowering the receiver clock does not remove this limitation.

Checks (ordinary tests use synthetic WLAN samples and do not open USB):

```sh
cargo test --all-targets
cargo clippy --target wasm32-unknown-unknown --lib -- -D warnings
trunk build --release
# Real Pluto: source delivery, WLAN graph, live gain/retune, and shutdown:
cargo test --test pluto_hardware -- --ignored --nocapture
```

The hardware check does not require ambient packets to decode. Browser WebUSB
permission and device access must still be granted from the page's Start RX
button; native hardware checks do not establish browser throughput.
