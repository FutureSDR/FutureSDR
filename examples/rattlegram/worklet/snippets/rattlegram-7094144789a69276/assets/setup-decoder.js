import DecoderNode from "../../../decoder-node.js";

async function getWebAudioMediaStream() {
  if (!window.navigator.mediaDevices) {
    throw new Error(
      "This browser does not support web audio or it is not enabled."
    );
  }

  try {
    const result = await window.navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: { exact: false }, noiseSuppression: { exact: false }, sampleRate: { exact: 48000 } },
      video: false,
    });

    return result;
  } catch (e) {
    switch (e.name) {
      case "NotAllowedError":
        throw new Error(
          "A recording device was found but has been disallowed for this application. Enable the device in the browser settings."
        );

      case "NotFoundError":
        throw new Error(
          "No recording device was found. Please attach a microphone and click Retry."
        );

      default:
        throw e;
    }
  }
}

export async function setupAudio(messageSetter) {
  const mediaStream = await getWebAudioMediaStream();

  const context = new window.AudioContext();
  const audioSource = context.createMediaStreamSource(mediaStream);

  let node;

  try {
    const response = await window.fetch("wasm-decoder_bg.wasm");
    const wasmBytes = await response.arrayBuffer();

    const processorUrl = "decoder-processor.js";
    try {
      await context.audioWorklet.addModule(processorUrl);
    } catch (e) {
      throw new Error(
        `Failed to load audio analyzer worklet at url: ${processorUrl}. Further info: ${e.message}`
      );
    }

    node = new DecoderNode(context, "DecoderProcessor");
    node.init(wasmBytes, messageSetter);
    audioSource.connect(node);
    node.connect(context.destination);
  } catch (err) {
    throw new Error(
      `Failed to load audio analyzer WASM module. Further info: ${err.message}`
    );
  }

  return { context, node };
}

