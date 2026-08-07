# FutureSDR WebGPU Spectrum

This browser-only example receives IQ samples from a HackRF or HydraSDR through
Seify's async WebUSB backend and computes a 32-frame, 2048-bin spectrum directly
with WGPU/WGSL. The GPU emits
linear mean power; the Prophecy spectrum and waterfall shaders convert it to dB
while rendering.

The async Seify source and WebGPU spectrum block run in separate Web Workers. The
GUI sink and the default WASM scheduler remain on the browser main thread.

WebGPU and WebUSB require a supported browser and a secure context (`localhost`
is sufficient for local development). Build or serve it with Trunk:

```sh
rustup target add wasm32-unknown-unknown
trunk serve
```

The worker build uses shared WebAssembly memory, so deployments outside Trunk's
development server must also send the `Cross-Origin-Opener-Policy: same-origin`
and `Cross-Origin-Embedder-Policy: require-corp` response headers configured in
`Trunk.toml`.
