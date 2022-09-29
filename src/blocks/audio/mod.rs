//! ## Audio Blocks
#[cfg(feature = "audio")]
mod audio_sink;
#[cfg(feature = "audio")]
pub use audio_sink::AudioSink;
#[cfg(feature = "audio")]
mod audio_source;
#[cfg(feature = "audio")]
pub use audio_source::AudioSource;

#[cfg(all(not(target_arch = "wasm32"), feature = "audio"))]
mod file_source;
#[cfg(all(not(target_arch = "wasm32"), feature = "audio"))]
pub use file_source::FileSource;

#[cfg(all(not(target_arch = "wasm32"), feature = "audio"))]
mod wav_sink;
#[cfg(all(not(target_arch = "wasm32"), feature = "audio"))]
pub use wav_sink::WavSink;
