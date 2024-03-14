use futuresdr::log::info;
use web_sys::wasm_bindgen;
use web_sys::wasm_bindgen::prelude::*;

use crate::Decoder;
use crate::DecoderResult;
use crate::OperationMode;

#[wasm_bindgen]
struct WasmDecoder {
    samples: Vec<f32>,
    decoder: Decoder,
}

#[wasm_bindgen]
impl WasmDecoder {
    pub fn new() -> Self {
        _ = console_log::init_with_level(futuresdr::log::Level::Debug);
        console_error_panic_hook::set_once();
        Self {
            samples: Vec::new(),
            decoder: Decoder::new(),
        }
    }

    pub fn process(&mut self, samples: Vec<f32>) {
        self.samples.extend_from_slice(&samples);
        // info!("samples len {}", self.samples.len());
        if self.samples.len() < 4096 {
            return;
        }
        if !self.decoder.feed(&std::mem::take(&mut self.samples)) {
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

