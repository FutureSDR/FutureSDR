use cpal::BufferSize;
use cpal::Stream;
use cpal::StreamConfig;
use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;
use futures::channel::oneshot;

use crate::channel::mpsc;
use crate::prelude::*;

/// Audio Sink.
#[derive(Block)]
pub struct AudioSink<I = DefaultCpuReader<f32>>
where
    I: CpuBufferReader<Item = f32>,
{
    #[input]
    input: I,
    input_channels: u16,
    sample_rate: u32,
    channels: u16,
    stream: Option<Stream>,
    min_buffer_size: usize,
    vec: Vec<f32>,
    terminated: Option<oneshot::Receiver<()>>,
    tx: Option<mpsc::Sender<Vec<f32>>>,
}

// cpal::Stream is !Send
#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl<I> Send for AudioSink<I> where I: CpuBufferReader<Item = f32> {}

const QUEUE_SIZE: usize = 5;
const STANDARD_RATES: [u32; 4] = [24000, 44100, 48000, 96000];

impl<I> AudioSink<I>
where
    I: CpuBufferReader<Item = f32>,
{
    fn supports_config(sample_rate: u32, channels: u16) -> bool {
        let Some(device) = cpal::default_host().default_output_device() else {
            return false;
        };

        let Ok(configs) = device.supported_output_configs() else {
            return false;
        };

        configs.into_iter().any(|config| {
            config.channels() == channels
                && sample_rate >= config.min_sample_rate()
                && sample_rate <= config.max_sample_rate()
        })
    }

    /// Create AudioSink block
    pub fn new(sample_rate: u32, channels: u16) -> Result<Self> {
        let output_channels = if Self::supports_config(sample_rate, channels) {
            channels
        } else if channels == 1 && Self::supports_config(sample_rate, 2) {
            warn!(
                "audio sink requested mono output at {} Hz, but only stereo is supported; duplicating samples to both channels",
                sample_rate
            );
            2
        } else {
            return Err(Error::InvalidParameter.into());
        };

        Ok(AudioSink {
            input: I::default(),
            input_channels: channels,
            sample_rate,
            channels: output_channels,
            stream: None,
            min_buffer_size: 2048,
            vec: Vec::new(),
            terminated: None,
            tx: None,
        })
    }
}

impl AudioSink<DefaultCpuReader<f32>> {
    /// Get default sample rate
    pub fn default_sample_rate() -> Option<u32> {
        Some(
            cpal::default_host()
                .default_output_device()?
                .default_output_config()
                .ok()?
                .sample_rate(),
        )
    }
    /// Get supported sample rates
    pub fn supported_sample_rates() -> Vec<u32> {
        if let Some(d) = cpal::default_host().default_output_device()
            && let Ok(configs) = d.supported_output_configs()
        {
            let mut v = Vec::new();
            for c in configs {
                let min = c.min_sample_rate();
                let max = c.max_sample_rate();
                if min >= 10000 {
                    v.push(min);
                }
                if max >= 10000 {
                    v.push(max);
                }

                v.extend(STANDARD_RATES.iter().filter(|x| *x >= &min && *x <= &max));
            }
            v.sort();
            v.dedup();
            return v;
        }
        Vec::new()
    }
}

#[doc(hidden)]
impl<I> Kernel for AudioSink<I>
where
    I: CpuBufferReader<Item = f32>,
{
    async fn init(&mut self, _m: &mut MessageOutputs, _b: &mut BlockMeta) -> Result<()> {
        let device = cpal::default_host()
            .default_output_device()
            .expect("no output device available");
        let duplicate_mono = self.input_channels == 1 && self.channels == 2;

        let config = StreamConfig {
            channels: self.channels,
            sample_rate: self.sample_rate,
            buffer_size: BufferSize::Default,
        };

        let (terminate, terminated) = oneshot::channel();
        let mut terminate = Some(terminate);
        self.terminated = Some(terminated);
        let (tx, rx) = mpsc::channel(QUEUE_SIZE);
        let mut iter: Option<Vec<f32>> = None;

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let mut i = 0;

                    while let Some(mut v) = iter.take().or_else(|| rx.try_recv().ok()) {
                        if v.is_empty() {
                            if let Some(t) = terminate.take() {
                                t.send(()).unwrap();
                            }
                            return;
                        }
                        if duplicate_mono {
                            let n = std::cmp::min(v.len(), (data.len() - i) / 2);
                            for j in 0..(n) {
                                data[i + 2 * j] = v[j];
                                data[i + 2 * j + 1] = v[j];
                            }
                            i += 2 * n;
                            if n < v.len() {
                                iter = Some(v.split_off(n));
                                debug_assert!(!iter.as_ref().unwrap().is_empty());
                                debug_assert_eq!(i, data.len());
                                return;
                            } else if i == data.len() {
                                return;
                            }
                        } else {
                            let n = std::cmp::min(v.len(), data.len() - i);
                            data[i..i + n].copy_from_slice(&v[..n]);
                            i += n;
                            if n < v.len() {
                                iter = Some(v.split_off(n));
                                debug_assert!(!iter.as_ref().unwrap().is_empty());
                                debug_assert_eq!(i, data.len());
                                return;
                            } else if i == data.len() {
                                return;
                            }
                        }
                    }
                },
                move |err| {
                    panic!("cpal stream error {err:?}");
                },
                None,
            )
            .expect("could not build output stream");
        // On Windows there is an issue in cpal with
        // shared devices, if the requested configuration
        // does not match the device configuration.
        // https://github.com/RustAudio/cpal/issues/593

        stream.play()?;

        self.tx = Some(tx);
        self.stream = Some(stream);

        Ok(())
    }

    async fn deinit(&mut self, _m: &mut MessageOutputs, _b: &mut BlockMeta) -> Result<()> {
        let _ = self.tx.as_mut().unwrap().send(Vec::new()).await;
        if let Some(t) = self.terminated.take() {
            _ = t.await;
        }
        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let i_len = i.len();

        self.vec.extend_from_slice(i);
        if self.vec.len() >= self.min_buffer_size || self.input.finished() {
            self.tx
                .as_mut()
                .unwrap()
                .send(std::mem::take(&mut self.vec))
                .await?;
        }

        self.input.consume(i_len);

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}
