import "./text-encoder.js";
import init, { WasmDecoder } from "./wasm-decoder.js";

class DecoderProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.port.onmessage = (event) => this.onmessage(event.data);
    this.decoder = null;
  }

  onmessage(event) {
    if (event.type === "send-wasm-module") {
      init(WebAssembly.compile(event.wasmBytes)).then(() => {
        this.port.postMessage({ type: 'wasm-module-loaded' });
      });
    } else if (event.type === 'init-decoder') {
      const { sampleRate } = event;
      this.decoder = WasmDecoder.new();
      console.log("decoder initialized, sample rate ", sampleRate);
      // this.samples = new Array(numAudioSamplesPerAnalysis).fill(0);
    }
  }

  process(inputs, outputs) {
    if (!this.decoder) {
      return true;
    }
    const inputChannels = inputs[0];
    const inputSamples = inputChannels[0];

    const result = this.decoder.process(inputSamples);
    if (result) {
      this.port.postMessage(result);
    }

    return true;
  }
}

registerProcessor("DecoderProcessor", DecoderProcessor);
