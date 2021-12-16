console.log("in worker (top)");



// function Sleep(milliseconds) {
//     return new Promise(resolve => setTimeout(resolve, milliseconds));
// }

// const runWasm = async () => {
//     await Sleep(5000);
//     const rustWasm = await init();
//     add_freq('#freq', '', -20, 10);
// };

// function waitForElement(){

//     if(typeof read_samples === 'function') {
//         console.log("present, loading");

//         runWasm();
//     } else {
//         console.log("not present, waiting");
//         setTimeout(waitForElement, 250);
//     }
// }

// waitForElement();


put_samples = function(s) {
    console.log("sending samples to main thread");
    postMessage(s);
}

var started = false;

onmessage = function(e) {
    if(e.data == "start" && !started) {
        started = true;
        console.log("starting worker");
        importScripts('rtl_open.js');
    } else {
        console.log("unknown message");
        console.log(e);
    }
}


// self.read_samples = function () {
//     // console.log("read samples called in woker");
//     postMessage("read_samples");

//     var promise = new Promise(function(resolve) {
//         resolve();
//     });

//     return promise;
// }

// self.onmessage = function(e) {
//     console.log("SETTING SAMPLES IN MSG HANDLER");
// }
// self.addEventListener("message", function() {
//     console.log("SETTING SAMPLES IN MSG HANDLER");
// });

// import init, {run_fg} from "./spectrum.js"


// const runWasm = async () => {
//     console.log("in runWasm");
//     const rustWasm = await init();
//     console.log("starting flowgraph");
//     await run_fg();
// };

// console.log("in worker");
// runWasm();
