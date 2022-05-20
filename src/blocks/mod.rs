//! ## Generic blocks
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [Apply] | Apply a function to each sample | ✅ |
//! | [ApplyNM] | ApplyNM a function to each N input samples and produce M output samples | ✅ |
//! | [Combine] | Apply a function to combine two streams into one | ✅ |
//! | [Filter] | Apply a function to filter samples | ✅ |
//!
//! ## DSP blocks
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [fir](FirBuilder) | Generic FIR filter | ✅ |
//! | [fft](FftBuilder) | Computes FFT | ✅ |
//!
//! ## Limiting blocks
//! | Block| Usage | WebAssembly? |
//! |---|---|---|
//! | [Throttle] | Limits graph sample rate | ❌ |
//! | [Head] | Stops graph after specified number of samples | ✅ |
//!
//! ## Source/sink blocks
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [FileSource] | Reads samples from a file | ❌ |
//! | [SoapySink](SoapySinkBuilder) | Transmit samples with a soapy device | ❌ |
//! | [SoapySource](SoapySourceBuilder) | Read samples from a soapy device | ❌ |
//! | [Source] | Repeatedly apply a function to generate samples | ✅ |
//! | [NullSource] | Generates a stream of zeros | ✅ |
//! | [FileSink] | Writes samples to a file | ❌ |
//! | [NullSink] | Drops samples | ✅ |
//! | [TagSink] | Drops samples, printing tags. | ✅ |
//! | [WavSink] | Writes samples to a WAV file | ❌ |
//!
//! ## Message blocks
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [MessageSource](MessageSourceBuilder) | Repeats a fixed message on an interval | ❌ |

mod apply;
pub use apply::Apply;

mod applynm;
pub use applynm::ApplyNM;

mod applyintoiter;
pub use applyintoiter::ApplyIntoIter;

pub mod audio;

#[cfg(not(target_arch = "wasm32"))]
mod blob_to_udp;
#[cfg(not(target_arch = "wasm32"))]
pub use blob_to_udp::BlobToUdp;

mod combine;
pub use combine::Combine;

mod console_sink;
pub use console_sink::ConsoleSink;

mod copy;
pub use copy::Copy;
mod copy_rand;
pub use copy_rand::{CopyRand, CopyRandBuilder};

mod filter;
pub use filter::Filter;

mod fir;
pub use fir::FirBuilder;

mod fft;
pub use fft::{Fft, FftBuilder};

#[cfg(not(target_arch = "wasm32"))]
mod file_sink;
#[cfg(not(target_arch = "wasm32"))]
pub use file_sink::FileSink;

#[cfg(not(target_arch = "wasm32"))]
mod file_source;
#[cfg(not(target_arch = "wasm32"))]
pub use file_source::FileSource;

mod finite_source;
pub use finite_source::FiniteSource;
mod head;
pub use head::Head;

mod iir;
pub use iir::{Iir, IirBuilder};

#[cfg(feature = "lttng")]
pub mod lttng;

mod message_burst;
pub use message_burst::{MessageBurst, MessageBurstBuilder};
mod message_copy;
pub use message_copy::{MessageCopy, MessageCopyBuilder};
mod message_sink;
pub use message_sink::{MessageSink, MessageSinkBuilder};

#[cfg(not(target_arch = "wasm32"))]
mod message_source;
#[cfg(not(target_arch = "wasm32"))]
pub use message_source::{MessageSource, MessageSourceBuilder};

mod null_sink;
pub use null_sink::NullSink;
mod null_source;
pub use null_source::NullSource;

#[cfg(feature = "soapy")]
mod soapy_snk;
#[cfg(feature = "soapy")]
pub use soapy_snk::{SoapySink, SoapySinkBuilder};
#[cfg(feature = "soapy")]
mod soapy_src;
#[cfg(feature = "soapy")]
pub use soapy_src::{SoapySource, SoapySourceBuilder};

mod source;
pub use source::Source;
mod split;
pub use split::Split;

mod tag_debug;
pub use tag_debug::TagDebug;

#[cfg(not(target_arch = "wasm32"))]
mod tcp_sink;
#[cfg(not(target_arch = "wasm32"))]
pub use tcp_sink::{TcpSink, TcpSinkBuilder};

#[cfg(not(target_arch = "wasm32"))]
mod tcp_source;
#[cfg(not(target_arch = "wasm32"))]
pub use tcp_source::{TcpSource, TcpSourceBuilder};

#[cfg(not(target_arch = "wasm32"))]
mod throttle;
#[cfg(not(target_arch = "wasm32"))]
pub use throttle::Throttle;

mod vector_sink;
pub use vector_sink::{VectorSink, VectorSinkBuilder};
mod vector_source;
pub use vector_source::{VectorSource, VectorSourceBuilder};

#[cfg(feature = "vulkan")]
mod vulkan;
#[cfg(feature = "vulkan")]
pub use vulkan::{Vulkan, VulkanBuilder};

#[cfg(target_arch = "wasm32")]
mod wasm_sdr;
#[cfg(target_arch = "wasm32")]
pub use wasm_sdr::WasmSdr;
#[cfg(target_arch = "wasm32")]
mod wasm_freq;
#[cfg(target_arch = "wasm32")]
pub use wasm_freq::WasmFreq;

#[cfg(not(target_arch = "wasm32"))]
mod websocket_sink;
#[cfg(not(target_arch = "wasm32"))]
pub use websocket_sink::{WebsocketSink, WebsocketSinkBuilder, WebsocketSinkMode};

#[cfg(feature = "wgpu")]
mod wgpu;
#[cfg(feature = "wgpu")]
pub use self::wgpu::Wgpu;

#[cfg(feature = "zeromq")]
pub mod zeromq;

#[cfg(feature = "zynq")]
mod zynq;
#[cfg(feature = "zynq")]
pub use zynq::{Zynq, ZynqBuilder};

#[cfg(feature = "zynq")]
mod zynq_sync;
#[cfg(feature = "zynq")]
pub use zynq_sync::{ZynqSync, ZynqSyncBuilder};
