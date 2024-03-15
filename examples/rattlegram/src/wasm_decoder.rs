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

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub enum DecoderMessage {
    Fail,
    Sync { cfo: f32, call_sign: String },
    Ping { cfo: f32, call_sign: String },
    Nope { cfo: f32, call_sign: String },
    HeapError,
    Done { bit_flips: i32, message: String },
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

    pub fn process(&mut self, samples: Vec<f32>) -> Option<String> {
        self.samples.extend_from_slice(&samples);
        if self.samples.len() < 1024 {
            return None;
        }
        if !self.decoder.feed(&std::mem::take(&mut self.samples)) {
            return None;
        }

        let status = self.decoder.process();
        let mut cfo = -1.0;
        let mut mode = OperationMode::Null;
        let mut call_sign = [0u8; 192];
        let mut payload = [0u8; 170];

        match status {
            DecoderResult::Okay => return None,
            DecoderResult::Fail => {
                info!("preamble fail");
                return Some(serde_json::to_string(&DecoderMessage::Fail).unwrap());
            }
            DecoderResult::Sync => {
                self.decoder.staged(&mut cfo, &mut mode, &mut call_sign);
                info!("SYNC:");
                info!("  CFO: {}", cfo);
                info!("  Mode: {:?}", mode);
                info!("  call sign: {}", String::from_utf8_lossy(&call_sign).trim_matches(char::from(0)));
                return Some(
                    serde_json::to_string(&DecoderMessage::Sync {
                        cfo,
                        call_sign: String::from_utf8_lossy(&call_sign).trim_matches(char::from(0)).to_string(),
                    })
                    .unwrap(),
                );
            }
            DecoderResult::Done => {
                let flips = self.decoder.fetch(&mut payload);
                info!("Bit flips: {}", flips);
                info!("Message: {}", String::from_utf8_lossy(&payload).trim_matches(char::from(0)));
                return Some(
                    serde_json::to_string(&DecoderMessage::Done {
                        bit_flips: flips,
                        message: String::from_utf8_lossy(&payload).trim_matches(char::from(0)).to_string(),
                    })
                    .unwrap(),
                );
            }
            DecoderResult::Heap => {
                info!("HEAP ERROR");
                return Some(serde_json::to_string(&DecoderMessage::HeapError).unwrap());
            }
            DecoderResult::Nope => {
                self.decoder.staged(&mut cfo, &mut mode, &mut call_sign);
                info!("NOPE:");
                info!("  CFO: {}", cfo);
                info!("  Mode: {:?}", mode);
                info!("  call sign: {}", String::from_utf8_lossy(&call_sign).trim_matches(char::from(0)));
                return Some(
                    serde_json::to_string(&DecoderMessage::Nope {
                        cfo,
                        call_sign: String::from_utf8_lossy(&call_sign).trim_matches(char::from(0)).to_string(),
                    })
                    .unwrap(),
                );
            }
            DecoderResult::Ping => {
                self.decoder.staged(&mut cfo, &mut mode, &mut call_sign);
                info!("PING:");
                info!("  CFO: {}", cfo);
                info!("  Mode: {:?}", mode);
                info!("  call sign: {}", String::from_utf8_lossy(&call_sign).trim_matches(char::from(0)));
                return Some(
                    serde_json::to_string(&DecoderMessage::Ping {
                        cfo,
                        call_sign: String::from_utf8_lossy(&call_sign).trim_matches(char::from(0)).to_string(),
                    })
                    .unwrap(),
                );
            }
            _ => {
                panic!("wrong decoder result");
            }
        }
    }
}
