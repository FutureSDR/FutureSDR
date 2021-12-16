console.log("in worker (top)");

self.put_samples = function(s) {
    console.log("sending samples to main thread");
    postMessage(s);
}

self.read_samples = function () {
    // console.log("read samples called in woker");
    postMessage("read_samples");

    var promise = new Promise(function(resolve) {
        resolve();
    });

    return promise;
}

self.onmessage = function(e) {
    console.log("SETTING SAMPLES IN MSG HANDLER");
}
onmessage = function(e) {
    console.log("SETTING SAMPLES IN MSG HANDLER");
}
self.addEventListener("message", function() {
    console.log("SETTING SAMPLES IN MSG HANDLER");
});

import init, {run_fg} from "./spectrum.js"


const runWasm = async () => {
    console.log("in runWasm");
    const rustWasm = await init();
    console.log("starting flowgraph");
    await run_fg();
};

console.log("in worker");
runWasm();
