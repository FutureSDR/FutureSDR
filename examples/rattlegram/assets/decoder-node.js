export default class DecoderNode extends AudioWorkletNode {
  init(wasmBytes, messageSetter) {
    this.messageSetter = messageSetter;
    this.port.onmessage = (event) => this.onmessage(event.data);
    this.port.postMessage({
      type: "send-wasm-module",
      wasmBytes,
    });
  }

  onprocessorerror(err) {
    console.log(
      `An error from AudioWorkletProcessor.process() occurred: ${err}`
    );
  };

  onmessage(event) {
    if (event.type === 'wasm-module-loaded') {
      this.port.postMessage({
        type: "init-decoder",
        sampleRate: this.context.sampleRate,
      });
    } else {
      this.messageSetter.message(event);
    }
  }
}
