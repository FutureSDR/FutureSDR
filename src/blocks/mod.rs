//! ## Block Library
//! ## Functional/Apply-style Blocks
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [Apply] | Apply a function to each sample. | ✅ |
//! | [ApplyIntoIter] | Apply a function on each input sample to create an iterator and output its values. | ✅ |
//! | [ApplyNM] | Apply a function to each N input samples, producing M output samples. | ✅ |
//! | [Combine] | Apply a function to combine two streams into one. | ✅ |
//! | [Filter] | Apply a function, returning an [Option] to allow filtering samples. | ✅ |
//! | [Source] | Repeatedly apply a function to generate samples. | ✅ |
//! | [Split] | Apply a function to split a stream. | ✅ |
//! | [FiniteSource] | Repeatedly apply a function to generate samples, using [Option] values to allow termination. | ✅ |
//!
//! ## DSP blocks
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [Fft](Fft) | Compute an FFT. | ✅ |
//! | [Fir](FirBuilder) | FIR filter and resampler. | ✅ |
//! | [Iir](IirBuilder) | IIR filter. | ✅ |
//!
//! ## Misc
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [ConsoleSink] | Log stream data with [log::info!]. | ✅ |
//! | [Head] | Copies only a given number of samples and stops. | ✅ |
//! | [NullSink] | Drops samples. | ✅ |
//! | [NullSource] | Generates a stream of zeros. | ✅ |
//! | [TagDebug] | Drop samples, printing tags. | ✅ |
//! | [Throttle] | Limit sample rate. | ❌ |
//! | [VectorSink] | Store received samples in vector. | ✅ |
//! | [VectorSource] | Stream samples from vector. | ✅ |
//!
//! ## Message Passing
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [MessageBurst] | Output a given number of messages in one burst and terminate. | ✅ |
//! | [MessageCopy] | Forward messages. | ✅ |
//! | [MessagePipe] | Push received messages into a channel. | ✅ |
//! | [MessageSink] | Black hole for messages. | ✅ |
//! | [MessageSource](MessageSourceBuilder) | Output the same message periodically. | ✅ |
//!
//! ## Performance Evaluation
//! | Block | Usage | WebAssembly? | Feature |
//! |---|---|---|---|
//! | [struct@Copy] | Copy input samples to the output. | ✅ | |
//! | [CopyRand] | Copy input samples to the output, forwarding only a randomly selected number of samples. | ❌ | |
//! | lttng::NullSource | Null source that calls an [lttng](https://lttng.org/) tracepoint for every batch of produced samples. | ❌ | lttng |
//! | lttng:NullSink | Null sink that calls an [lttng](https://lttng.org/) tracepoint for every batch of received samples. | ❌ | lttng |
//!
//! ## I/O
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [BlobToUdp] | Push [Blobs](crate::runtime::Pmt::Blob) into a UDP socket.| ❌ |
//! | [FileSink] | Write samples to a file. | ❌ |
//! | [FileSource] | Read samples from a file. | ❌ |
//! | [TcpSource] | Reads samples from a TCP socket. | ❌ |
//! | [TcpSink] | Push samples into a TCP socket. | ❌ |
//! | [WebsocketSink] | Push samples in a WebSocket. | ❌ |
//! | [zeromq::PubSink] | Push samples into [ZeroMQ](https://zeromq.org/) socket. | ❌ |
//! | [zeromq::SubSource] | Read samples from [ZeroMQ](https://zeromq.org/) socket. | ❌ |
//!
//! ## SDR Hardware (requires `soapy` feature)
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [SoapySink](SoapySinkBuilder) | Transmit samples with a Soapy SDR device. | ❌ |
//! | [SoapySource](SoapySourceBuilder) | Receive samples from a Soapy SDR device. | ❌ |
//!
//! ## Hardware Acceleration
//! | Block | Usage | WebAssembly? | Feature |
//! |---|---|---|---|
//! | [Vulkan] | Interface GPU w/ Vulkan. | ❌ | `vulkan` |
//! | [Wgpu] | Interface GPU w/ native API. | ✅ | `wgpu` |
//! | [Zynq] | Interface Zynq FPGA w/ AXI DMA (async mode). | ❌ | `zynq` |
//! | [ZynqSync] | Interface Zynq FPGA w/ AXI DMA (sync mode). | ❌ | `zynq` |
//!
//! ## WASM-specific (target `wasm32-unknown-unknown`)
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | WasmSdr | Receive samples from web world. | ✅ |
//! | WasmWsSink | Send samples via a WebSocket. | ✅ |
//! | WasmFreq | Push samples to a GUI sink. | ✅ |
//!
//! ## Audio (requires `audio` feature)
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [AudioSink](audio::AudioSink) | Audio sink. | ❌ |
//! | [AudioSource](audio::AudioSource) | Audio source. | ❌ |
//! | [FileSource](audio::FileSource) | Read an audio file and output its samples. | ❌ |
//! | [Oscillator](audio::Oscillator) | Create tone. | ✅ |
//! | [WavSink](audio::WavSink) | Writes samples to a WAV file | ❌ |
//!

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
pub use fir::Fir;
pub use fir::FirBuilder;

mod fft;
pub use fft::Fft;
pub use fft::FftDirection;

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
pub use message_copy::MessageCopy;
mod message_pipe;
pub use message_pipe::MessagePipe;
mod message_sink;
pub use message_sink::MessageSink;

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
pub use tcp_sink::TcpSink;

#[cfg(not(target_arch = "wasm32"))]
mod tcp_source;
#[cfg(not(target_arch = "wasm32"))]
pub use tcp_source::TcpSource;

#[cfg(not(target_arch = "wasm32"))]
mod throttle;
#[cfg(not(target_arch = "wasm32"))]
pub use throttle::Throttle;

mod vector_sink;
pub use vector_sink::{VectorSink, VectorSinkBuilder};
mod vector_source;
pub use vector_source::VectorSource;

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
pub use zynq::Zynq;

#[cfg(feature = "zynq")]
mod zynq_sync;
#[cfg(feature = "zynq")]
pub use zynq_sync::ZynqSync;

#[cfg(target_arch = "wasm32")]
mod wasm_ws_sink;
#[cfg(target_arch = "wasm32")]
pub use wasm_ws_sink::WasmWsSink;
