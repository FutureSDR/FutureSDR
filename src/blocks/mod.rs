//! ## Block Library
//! ## Functional/Apply-style Blocks
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [Apply](crate::blocks::Apply) | Apply a function to each sample. | ✅ |
//! | [ApplyIntoIter](crate::blocks::ApplyIntoIter) | Apply a function on each input sample to create an iterator and output its values. | ✅ |
//! | [ApplyNM](crate::blocks::ApplyNM) | Apply a function to each N input samples, producing M output samples. | ✅ |
//! | [Combine](crate::blocks::Combine) | Apply a function to combine two streams into one. | ✅ |
//! | [Filter](crate::blocks::Filter) | Apply a function, returning an [Option] to allow filtering samples. | ✅ |
//! | [Sink](crate::blocks::Sink) | Apply a function to received samples. | ✅ |
//! | [Source](crate::blocks::Source) | Repeatedly apply a function to generate samples. | ✅ |
//! | [Split](crate::blocks::Split) | Apply a function to split a stream. | ✅ |
//! | [FiniteSource](crate::blocks::FiniteSource) | Repeatedly apply a function to generate samples, using [Option] values to allow termination. | ✅ |
//!
//! ## Streams
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [StreamDeinterleaver](crate::blocks::StreamDeinterleaver) | Stream Deinterleave | ✅ |
//! | [StreamDuplicator](crate::blocks::StreamDuplicator) | Stream Duplicator | ✅ |
//!
//! ## DSP blocks
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [Fft](crate::blocks::Fft) | Compute an FFT. | ✅ |
//! | [Fir](crate::blocks::FirBuilder) | FIR filter and resampler. | ✅ |
//! | [Iir](crate::blocks::IirBuilder) | IIR filter. | ✅ |
//! | [PfbArbResampler](crate::blocks::PfbArbResampler) | Polyphase Arbitrary Rate Resampler | ✅ |
//! | [PfbChannelizer](crate::blocks::PfbChannelizer) | Polyphase Channelizer | ✅ |
//! | [PfbSynthesizer](crate::blocks::PfbSynthesizer) | Polyphase Synthesizer | ✅ |
//! | [XlatingFir](crate::blocks::XlatingFir) | Xlating FIR filter and decimator. | ✅ |
//!
//! ## Misc
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [Delay](crate::blocks::Delay) | Delays samples. | ✅ |
//! | [Head](crate::blocks::Head) | Copies only a given number of samples and stops. | ✅ |
//! | [MovingAvg](crate::blocks::MovingAvg) | Applies an exponential moving average over a window samples. | ✅ |
//! | [NullSink](crate::blocks::NullSink) | Drops samples. | ✅ |
//! | [NullSource](crate::blocks::NullSource) | Generates a stream of zeros. | ✅ |
//! | [Selector](crate::blocks::Selector) | Forward the input stream with a given index to the output stream with a given index. | ✅ |
//! | [TagDebug](crate::blocks::TagDebug) | Drop samples, printing tags. | ✅ |
//! | [Throttle](crate::blocks::Throttle) | Limit sample rate. | ✅ |
//! | [VectorSink](crate::blocks::VectorSink) | Store received samples in vector. | ✅ |
//! | [VectorSource](crate::blocks::VectorSource) | Stream samples from vector. | ✅ |
//!
//! ## Message Passing
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [MessageAnnotator](crate::blocks::MessageAnnotator) | Wrap every message in a DictStrPmt and add fixed additional fields, to facilitate multiplexing w/o losing the source association | ✅ |
//! | [MessageApply](crate::blocks::MessageApply) | Apply a function to each message, emitting the result as a new message. | ✅ |
//! | [MessageBurst](crate::blocks::MessageBurst) | Output a given number of messages in one burst and terminate. | ✅ |
//! | [MessageCopy](crate::blocks::MessageCopy) | Forward messages. | ✅ |
//! | [MessagePipe](crate::blocks::MessagePipe) | Push received messages into a channel. | ✅ |
//! | [MessageSink](crate::blocks::MessageSink) | Black hole for messages. | ✅ |
//! | [MessageSource](crate::blocks::MessageSourceBuilder) | Output the same message periodically. | ✅ |
//!
//! ## Performance Evaluation
//! | Block | Usage | WebAssembly? | Feature |
//! |---|---|---|---|
//! | [Copy](crate::blocks::Copy) | Copy input samples to the output. | ✅ | |
//!
//! ## I/O
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [BlobToUdp](crate::blocks::BlobToUdp) | Push [blobs](crate::runtime::Pmt::Blob) into a UDP socket. | ❌ |
//! | [ChannelSource](crate::blocks::ChannelSource) | Push samples through a channel into a stream connection. | ✅ |
//! | [ChannelSink](crate::blocks::ChannelSink) | Read samples from Flowgraph and send them into a channel | ✅ |
//! | [FileSink](crate::blocks::FileSink) | Write samples to a file. | ❌ |
//! | [FileSource](crate::blocks::FileSource) | Read samples from a file. | ❌ |
//! | [TcpSource](crate::blocks::TcpSource) | Reads samples from a TCP socket. | ❌ |
//! | [TcpSink](crate::blocks::TcpSink) | Push samples into a TCP socket. | ❌ |
//! | [UdpSource](crate::blocks::UdpSource) | Reads samples from a UDP socket. | ❌ |
//! | [WebsocketSink](crate::blocks::WebsocketSink) | Push samples in a WebSocket. | ❌ |
//! | [WebsocketPmtSink](crate::blocks::WebsocketPmtSink) | Push samples from Pmts a WebSocket. | ❌ |
//! | `zeromq::PubSink` | Push samples into [ZeroMQ](https://zeromq.org/) socket. | ❌ |
//! | `zeromq::SubSource` | Read samples from [ZeroMQ](https://zeromq.org/) socket. | ❌ |
//!
//! ## SDR Hardware
//! | Block | Usage | Feature | WebAssembly? |
//! |---|---|---|---|
//! | `seify::Sink` | Transmit samples with a Seify device. | seify | ❌ |
//! | `seify::Source` | Receive samples from a Seify device. | seify | ❌ |
//!
//! ## Hardware Acceleration
//! | Block | Usage | WebAssembly? | Feature |
//! |---|---|---|---|
//! | `Vulkan` | Interface GPU w/ Vulkan. | ❌ | `vulkan` |
//! | `Wgpu` | Interface GPU w/ native API. | ✅ | `wgpu` |
//! | `Zynq` | Interface Zynq FPGA w/ AXI DMA (async mode). | ❌ | `zynq` |
//! | `ZynqSync` | Interface Zynq FPGA w/ AXI DMA (sync mode). | ❌ | `zynq` |
//!
//! ## WASM-specific (target `wasm32-unknown-unknown`)
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | HackRf | WASM + WebUSB source for HackRF. | ✅ |
//! | WasmWsSink | Send samples via a WebSocket. | ✅ |
//!
//! ## Signal Sources
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | [SignalSource](crate::blocks::SignalSourceBuilder) | Create signals (sin, cos, square). | ✅ |
//!
//! ## Audio (requires `audio` feature)
//! | Block | Usage | WebAssembly? |
//! |---|---|---|
//! | `audio::AudioSink` | Audio sink. | ❌ |
//! | `audio::AudioSource` | Audio source. | ❌ |
//! | `audio::FileSource` | Read an audio file and output its samples. | ❌ |
//! | `audio::WavSink` | Writes samples to a WAV file | ❌ |
//!

mod apply;
pub use apply::Apply;
mod applyintoiter;
pub use applyintoiter::ApplyIntoIter;
mod applynm;
pub use applynm::ApplyNM;
#[cfg(feature = "audio")]
pub mod audio;
#[cfg(not(target_arch = "wasm32"))]
mod blob_to_udp;
#[cfg(not(target_arch = "wasm32"))]
pub use blob_to_udp::BlobToUdp;
mod channel_sink;
pub use channel_sink::ChannelSink;
mod channel_source;
pub use channel_source::ChannelSource;
mod combine;
pub use combine::Combine;
mod copy;
pub use copy::Copy;
mod delay;
pub use delay::Delay;
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
mod filter;
pub use filter::Filter;
mod finite_source;
pub use finite_source::FiniteSource;
mod fir;
pub use fir::Fir;
pub use fir::FirBuilder;
mod head;
pub use head::Head;
mod iir;
pub use iir::Iir;
pub use iir::IirBuilder;
mod message_annotator;
pub use message_annotator::MessageAnnotator;
mod message_apply;
pub use message_apply::MessageApply;
mod message_burst;
pub use message_burst::MessageBurst;
mod message_copy;
pub use message_copy::MessageCopy;
mod message_pipe;
pub use message_pipe::MessagePipe;
mod message_sink;
pub use message_sink::MessageSink;
#[cfg(not(target_arch = "wasm32"))]
mod message_source;
#[cfg(not(target_arch = "wasm32"))]
pub use message_source::MessageSource;
#[cfg(not(target_arch = "wasm32"))]
pub use message_source::MessageSourceBuilder;
mod moving_avg;
pub use moving_avg::MovingAvg;
mod null_sink;
pub use null_sink::NullSink;
mod null_source;
pub use null_source::NullSource;
mod pfb;
pub use pfb::arb_resampler::PfbArbResampler;
pub use pfb::channelizer::PfbChannelizer;
pub use pfb::synthesizer::PfbSynthesizer;
/// Seify hardware driver blocks
#[cfg(all(feature = "seify", not(target_arch = "wasm32")))]
pub mod seify;
mod selector;
pub use selector::DropPolicy as SelectorDropPolicy;
pub use selector::Selector;
pub mod signal_source;
pub use signal_source::FixedPointPhase;
pub use signal_source::SignalSource;
pub use signal_source::SignalSourceBuilder;
mod sink;
pub use sink::Sink;
mod source;
pub use source::Source;
mod split;
pub use split::Split;
mod stream_deinterleaver;
pub use stream_deinterleaver::StreamDeinterleaver;
mod stream_duplicator;
pub use stream_duplicator::StreamDuplicator;
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
mod throttle;
pub use throttle::Throttle;
#[cfg(not(target_arch = "wasm32"))]
mod udp_source;
#[cfg(not(target_arch = "wasm32"))]
pub use udp_source::UdpSource;
mod vector_sink;
pub use vector_sink::VectorSink;
mod vector_source;
pub use vector_source::VectorSource;
#[cfg(feature = "vulkan")]
mod vulkan;
#[cfg(feature = "vulkan")]
pub use vulkan::Vulkan;
/// WASM-specific blocks (target wasm32-unknown-unknown)
#[cfg(target_arch = "wasm32")]
pub mod wasm;
#[cfg(not(target_arch = "wasm32"))]
mod websocket_pmt_sink;
#[cfg(not(target_arch = "wasm32"))]
pub use websocket_pmt_sink::WebsocketPmtSink;
#[cfg(not(target_arch = "wasm32"))]
mod websocket_sink;
#[cfg(not(target_arch = "wasm32"))]
pub use websocket_sink::WebsocketSink;
#[cfg(not(target_arch = "wasm32"))]
pub use websocket_sink::WebsocketSinkBuilder;
#[cfg(not(target_arch = "wasm32"))]
pub use websocket_sink::WebsocketSinkMode;
pub mod xlating_fir;
pub use xlating_fir::XlatingFir;
#[cfg(feature = "wgpu")]
mod wgpu;
#[cfg(feature = "wgpu")]
pub use self::wgpu::Wgpu;
#[cfg(feature = "zeromq")]
pub mod zeromq;
#[cfg(all(feature = "zynq", target_os = "linux"))]
mod zynq;
#[cfg(all(feature = "zynq", target_os = "linux"))]
pub use zynq::Zynq;
#[cfg(all(feature = "zynq", target_os = "linux"))]
mod zynq_sync;
#[cfg(all(feature = "zynq", target_os = "linux"))]
pub use zynq_sync::ZynqSync;
