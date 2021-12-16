import init, {run_fg} from "./wasm.js"

const runWasm = async () => {
    const rustWasm = await init();
    await run_fg();
};

runWasm();
