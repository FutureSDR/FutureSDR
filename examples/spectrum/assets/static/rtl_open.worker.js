/**
 * @license
 * Copyright 2015 The Emscripten Authors
 * SPDX-License-Identifier: MIT
 */

// Pthread Web Worker startup routine:
// This is the entry point file that is loaded first by each Web Worker
// that executes pthreads on the Emscripten application.

// Thread-local:
var initializedJS = false; // Guard variable for one-time init of the JS state (currently only embind types registration)

var Module = {};

function threadPrintErr() {
  var text = Array.prototype.slice.call(arguments).join(' ');
  console.error(text);
}
function threadAlert() {
  var text = Array.prototype.slice.call(arguments).join(' ');
  postMessage({cmd: 'alert', text: text, threadId: Module['_pthread_self']()});
}
var err = threadPrintErr;
this.alert = threadAlert;

Module['instantiateWasm'] = function(info, receiveInstance) {
  // Instantiate from the module posted from the main thread.
  // We can just use sync instantiation in the worker.
  var instance = new WebAssembly.Instance(Module['wasmModule'], info);
  // TODO: Due to Closure regression https://github.com/google/closure-compiler/issues/3193,
  // the above line no longer optimizes out down to the following line.
  // When the regression is fixed, we can remove this if/else.
  receiveInstance(instance);
  // We don't need the module anymore; new threads will be spawned from the main thread.
  Module['wasmModule'] = null;
  return instance.exports;
};

// When using postMessage to send an object, it is processed by the structured clone algorithm.
// The prototype, and hence methods, on that object is then lost. This function adds back the lost prototype.
// This does not work with nested objects that has prototypes, but it suffices for WasmSourceMap and WasmOffsetConverter.
function resetPrototype(constructor, attrs) {
  var object = Object.create(constructor.prototype);
  for (var key in attrs) {
    if (attrs.hasOwnProperty(key)) {
      object[key] = attrs[key];
    }
  }
  return object;
}

var wasmSourceMapData;
var wasmOffsetData;

function moduleLoaded() {
  wasmSourceMap = resetPrototype(Module['WasmSourceMap'], wasmSourceMapData);
  wasmOffsetConverter = resetPrototype(Module['WasmOffsetConverter'], wasmOffsetData);
}

this.onmessage = function(e) {
  try {
    if (e.data.cmd === 'load') { // Preload command that is called once per worker to parse and load the Emscripten code.

      // Module and memory were sent from main thread
      Module['wasmModule'] = e.data.wasmModule;

      Module['wasmMemory'] = e.data.wasmMemory;

      wasmSourceMapData = e.data.wasmSourceMap;
      wasmOffsetData = e.data.wasmOffsetConverter;

      Module['buffer'] = Module['wasmMemory'].buffer;

      Module['ENVIRONMENT_IS_PTHREAD'] = true;

      if (typeof e.data.urlOrBlob === 'string') {
        importScripts(e.data.urlOrBlob);
      } else {
        var objectUrl = URL.createObjectURL(e.data.urlOrBlob);
        importScripts(objectUrl);
        URL.revokeObjectURL(objectUrl);
      }

      // MINIMAL_RUNTIME always compiled Wasm (&Wasm2JS) asynchronously, even in pthreads. But
      // regular runtime and asm.js are loaded synchronously, so in those cases
      // we are now loaded, and can post back to main thread.
      moduleLoaded();

    } else if (e.data.cmd === 'objectTransfer') {
      Module['PThread'].receiveObjectTransfer(e.data);
    } else if (e.data.cmd === 'run') {
      // This worker was idle, and now should start executing its pthread entry
      // point.
      // performance.now() is specced to return a wallclock time in msecs since
      // that Web Worker/main thread launched. However for pthreads this can
      // cause subtle problems in emscripten_get_now() as this essentially
      // would measure time from pthread_create(), meaning that the clocks
      // between each threads would be wildly out of sync. Therefore sync all
      // pthreads to the clock on the main browser thread, so that different
      // threads see a somewhat coherent clock across each of them
      // (+/- 0.1msecs in testing).
      Module['__performance_now_clock_drift'] = performance.now() - e.data.time;

      // Pass the thread address inside the asm.js scope to store it for fast access that avoids the need for a FFI out.
      Module['__emscripten_thread_init'](e.data.threadInfoStruct, /*isMainBrowserThread=*/0, /*isMainRuntimeThread=*/0);

      // Establish the stack frame for this thread in global scope
      // The stack grows downwards
      var max = e.data.stackBase;
      var top = e.data.stackBase + e.data.stackSize;
      // Also call inside JS module to set up the stack frame for this pthread in JS module scope
      Module['establishStackSpace'](top, max);
      Module['PThread'].receiveObjectTransfer(e.data);
      Module['PThread'].threadInit();

      // Embind must initialize itself on all threads, as it generates support JS.
      // We only do this once per worker since they get reused
      if (!initializedJS) {
        Module['___embind_register_native_and_builtin_types']();
        initializedJS = true;
      }

      try {
        // pthread entry points are always of signature 'void *ThreadMain(void *arg)'
        // Native codebases sometimes spawn threads with other thread entry point signatures,
        // such as void ThreadMain(void *arg), void *ThreadMain(), or void ThreadMain().
        // That is not acceptable per C/C++ specification, but x86 compiler ABI extensions
        // enable that to work. If you find the following line to crash, either change the signature
        // to "proper" void *ThreadMain(void *arg) form, or try linking with the Emscripten linker
        // flag -s EMULATE_FUNCTION_POINTER_CASTS=1 to add in emulation for this x86 ABI extension.
        var result = Module['invokeEntryPoint'](e.data.start_routine, e.data.arg);

        if (Module['keepRuntimeAlive']()) {
          Module['PThread'].setExitStatus(result);
        } else {
          Module['PThread'].threadExit(result);
        }
      } catch(ex) {
        if (ex === 'Canceled!') {
          Module['PThread'].threadCancel();
        } else if (ex != 'unwind') {
          // ExitStatus not present in MINIMAL_RUNTIME
          if (ex instanceof Module['ExitStatus']) {
            if (Module['keepRuntimeAlive']()) {
            } else {
              Module['PThread'].threadExit(ex.status);
            }
          }
          else
          {
            Module['PThread'].threadExit(-2);
            throw ex;
          }
        }
      }
    } else if (e.data.cmd === 'cancel') { // Main thread is asking for a pthread_cancel() on this thread.
      if (Module['_pthread_self']()) {
        Module['PThread'].threadCancel();
      }
    } else if (e.data.target === 'setimmediate') {
      // no-op
    } else if (e.data.cmd === 'processThreadQueue') {
      if (Module['_pthread_self']()) { // If this thread is actually running?
        Module['_emscripten_current_thread_process_queued_calls']();
      }
    } else {
      err('worker.js received unknown command ' + e.data.cmd);
      err(e.data);
    }
  } catch(ex) {
    err('worker.js onmessage() captured an uncaught exception: ' + ex);
    if (ex && ex.stack) err(ex.stack);
    throw ex;
  }
};

// Node.js support
if (typeof process === 'object' && typeof process.versions === 'object' && typeof process.versions.node === 'string') {
  // Create as web-worker-like an environment as we can.
  self = {
    location: {
      href: __filename
    }
  };

  var onmessage = this.onmessage;

  var nodeWorkerThreads = require('worker_threads');

  global.Worker = nodeWorkerThreads.Worker;

  var parentPort = nodeWorkerThreads.parentPort;

  parentPort.on('message', function(data) {
    onmessage({ data: data });
  });

  var nodeFS = require('fs');

  var nodeRead = function(filename) {
    return nodeFS.readFileSync(filename, 'utf8');
  };

  function globalEval(x) {
    global.require = require;
    global.Module = Module;
    eval.call(null, x);
  }

  importScripts = function(f) {
    globalEval(nodeRead(f));
  };

  postMessage = function(msg) {
    parentPort.postMessage(msg);
  };

  if (typeof performance === 'undefined') {
    performance = {
      now: function() {
        return Date.now();
      }
    };
  }
}


