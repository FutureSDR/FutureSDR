import init, {run_fg} from "./futuresdr.js"

const runWasm = async () => {
    const rustWasm = await init();
    await run_fg();
};

runWasm();
