import {
  initSync,
  futuresdr_wasm_local_domain_worker_entry,
  futuresdr_wasm_scheduler_worker_entry,
} from "./futuresdr_app.js";

const THREAD_STACK_SIZE = 1024 * 1024;

self.onerror = (event) => {
  console.error("FutureSDR worker error", event.message, event.error);
};

self.onunhandledrejection = (event) => {
  console.error("FutureSDR worker unhandled rejection", event.reason);
};

self.onmessage = (event) => {
  const data = event.data;
  if (!data) {
    return;
  }

  try {
    initSync({
      module: data.module,
      memory: data.memory,
      thread_stack_size: THREAD_STACK_SIZE,
    });

    if (data.type === "futuresdr-wasm-scheduler-init") {
      futuresdr_wasm_scheduler_worker_entry(data.executor_id, data.worker_index);
    } else if (data.type === "futuresdr-wasm-local-domain-init") {
      futuresdr_wasm_local_domain_worker_entry(data.domain_id);
    }
  } catch (e) {
    console.error("FutureSDR worker init failed", data, e);
    throw e;
  }
};
