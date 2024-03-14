import init, { WasmDecoder } from "./wasm-decoder.js";

class DecoderProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.sampleRate = null;
    this.port.onmessage = (event) => this.onmessage(event.data);
    this.decoder = null;
  }

  onmessage(event) {
    if (event.type === "send-wasm-module") {
      console.log("wasm compile");
      init(WebAssembly.compile(event.wasmBytes)).then(() => {
        console.log("compile done posting message");
        this.port.postMessage({ type: 'wasm-module-loaded' });
      });
    } else if (event.type === 'init-decoder') {
      console.log("decoder init start");
      const { sampleRate } = event;
      this.sampleRate = sampleRate;
      this.decoder = WasmDecoder.new();
      console.log("decoder initialized");
      // this.samples = new Array(numAudioSamplesPerAnalysis).fill(0);
    }
  }

  process(inputs, outputs) {
    if (!this.decoder) {
      console.log("not yet init, not processing");
      return true;
    }
    console.log("processing");
    const inputChannels = inputs[0];
    const inputSamples = inputChannels[0];

    this.decoder.process(inputSamples);
    // const result = this.detector.detect_pitch(this.samples);
    //
    // if (result !== 0) {
    //   this.port.postMessage({ type: "pitch", pitch: result });
    // }

    return true;
  }
}

registerProcessor("DecoderProcessor", DecoderProcessor);
