import DecoderNode from "../../../decoder-node.js";

async function getWebAudioMediaStream() {
  if (!window.navigator.mediaDevices) {
    throw new Error(
      "This browser does not support web audio or it is not enabled."
    );
  }

  try {
    const result = await window.navigator.mediaDevices.getUserMedia({
      audio: true,
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

// export async function setupAudio(onRxCallback) {
export async function setupAudio() {
  const onRxCallback = (a) => a;
  const mediaStream = await getWebAudioMediaStream();

  const context = new window.AudioContext();
  const audioSource = context.createMediaStreamSource(mediaStream);

  let node;

  try {
    // Fetch the WebAssembly module that performs pitch detection.
    const response = await window.fetch("wasm-decoder_bg.wasm");
    console.log("fetched")
    const wasmBytes = await response.arrayBuffer();
    console.log("bytes")

    // Add our audio processor worklet to the context.
    const processorUrl = "decoder-processor.js";
    // try {
      console.log("addmodule")
      console.log(context)
      console.log(context.audioWorklet)
      await context.audioWorklet.addModule(processorUrl);
    // } catch (e) {
    //   throw new Error(
    //     `Failed to load audio analyzer worklet at url: ${processorUrl}. Further info: ${e.message}`
    //   );
    // }

    // Create the AudioWorkletNode which enables the main JavaScript thread to
    // communicate with the audio processor (which runs in a Worklet).
    node = new DecoderNode(context, "DecoderProcessor");

    // numAudioSamplesPerAnalysis specifies the number of consecutive audio samples that
    // the pitch detection algorithm calculates for each unit of work. Larger values tend
    // to produce slightly more accurate results but are more expensive to compute and
    // can lead to notes being missed in faster passages i.e. where the music note is
    // changing rapidly. 1024 is usually a good balance between efficiency and accuracy
    // for music analysis.
    // const numAudioSamplesPerAnalysis = 1024;

    // Send the Wasm module to the audio node which in turn passes it to the
    // processor running in the Worklet thread. Also, pass any configuration
    // parameters for the Wasm detection algorithm.
    node.init(wasmBytes, onRxCallback);

    // Connect the audio source (microphone output) to our analysis node.
    audioSource.connect(node);

    // Connect our analysis node to the output. Required even though we do not
    // output any audio. Allows further downstream audio processing or output to
    // occur.
    node.connect(context.destination);
  } catch (err) {
    throw new Error(
      `Failed to load audio analyzer WASM module. Further info: ${err.message}`
    );
  }

  return { context, node };
}

