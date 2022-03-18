#[cfg(feature = "cpal")]
mod audio_sink;
#[cfg(feature = "cpal")]
pub use audio_sink::AudioSink;
#[cfg(feature = "cpal")]
mod audio_source;
#[cfg(feature = "cpal")]
pub use audio_source::AudioSource;

#[cfg(all(not(target_arch = "wasm32"), feature = "rodio"))]
mod file_source;
#[cfg(all(not(target_arch = "wasm32"), feature = "rodio"))]
pub use file_source::FileSource;
#[cfg(all(not(target_arch = "wasm32"), feature = "rodio"))]
mod oscillator;
#[cfg(all(not(target_arch = "wasm32"), feature = "rodio"))]
pub use oscillator::Oscillator;

#[cfg(all(not(target_arch = "wasm32"), feature = "hound"))]
mod wav_sink;
#[cfg(all(not(target_arch = "wasm32"), feature = "hound"))]
pub use wav_sink::WavSink;
