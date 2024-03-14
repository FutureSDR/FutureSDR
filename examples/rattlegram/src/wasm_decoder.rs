use futuresdr::log::info;
use web_sys::wasm_bindgen;
use web_sys::wasm_bindgen::prelude::*;

use crate::Decoder;
use crate::DecoderResult;
use crate::OperationMode;

#[wasm_bindgen]
struct WasmDecoder {
    decoder: Decoder,
}

#[wasm_bindgen]
impl WasmDecoder {
    pub fn new() -> Self {
        Self {
            decoder: Decoder::new(),
        }
    }

    pub fn process(&mut self, samples: Vec<f32>) {
        if !self.decoder.feed(&samples) {
            return;
        }

        let status = self.decoder.process();
        let mut cfo = -1.0;
        let mut mode = OperationMode::Null;
        let mut call_sign = [0u8; 192];
        let mut payload = [0u8; 170];

        match status {
            DecoderResult::Okay => {}
            DecoderResult::Fail => {
                info!("preamble fail");
            }
            DecoderResult::Sync => {
                self.decoder.staged(&mut cfo, &mut mode, &mut call_sign);
                info!("SYNC:");
                info!("  CFO: {}", cfo);
                info!("  Mode: {:?}", mode);
                info!("  call sign: {}", String::from_utf8_lossy(&call_sign));
            }
            DecoderResult::Done => {
                let flips = self.decoder.fetch(&mut payload);
                info!("Bit flips: {}", flips);
                info!("Message: {}", String::from_utf8_lossy(&payload));
            }
            DecoderResult::Heap => {
                info!("HEAP ERROR");
            }
            DecoderResult::Nope => {
                self.decoder.staged(&mut cfo, &mut mode, &mut call_sign);
                info!("NOPE:");
                info!("  CFO: {}", cfo);
                info!("  Mode: {:?}", mode);
                info!("  call sign: {}", String::from_utf8_lossy(&call_sign));
            }
            DecoderResult::Ping => {
                self.decoder.staged(&mut cfo, &mut mode, &mut call_sign);
                info!("PING:");
                info!("  CFO: {}", cfo);
                info!("  Mode: {:?}", mode);
                info!("  call sign: {}", String::from_utf8_lossy(&call_sign));
            }
            _ => {
                panic!("wrong decoder result");
            }
        }
    }
}

