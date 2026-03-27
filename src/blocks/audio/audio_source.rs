use cpal::BufferSize;
use cpal::Stream;
use cpal::StreamConfig;
use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;
use futures::StreamExt;
use futures::channel::mpsc;

use crate::prelude::*;

/// Audio Source.
#[derive(Block)]
pub struct AudioSource<O = DefaultCpuWriter<f32>>
where
    O: CpuBufferWriter<Item = f32>,
{
    #[output]
    output: O,
    output_channels: u16,
    sample_rate: u32,
    channels: u16,
    stream: Option<Stream>,
    rx: Option<mpsc::UnboundedReceiver<Vec<f32>>>,
    buff: Option<(Vec<f32>, usize)>,
}

// cpal::Stream is !Send
#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl<O> Send for AudioSource<O> where O: CpuBufferWriter<Item = f32> {}

impl<O> AudioSource<O>
where
    O: CpuBufferWriter<Item = f32>,
{
    fn supports_config(sample_rate: u32, channels: u16) -> bool {
        let Some(device) = cpal::default_host().default_input_device() else {
            return false;
        };

        let Ok(configs) = device.supported_input_configs() else {
            return false;
        };

        configs.into_iter().any(|config| {
            config.channels() == channels
                && sample_rate >= config.min_sample_rate()
                && sample_rate <= config.max_sample_rate()
        })
    }

    /// Create AudioSource block
    pub fn new(sample_rate: u32, channels: u16) -> Result<Self> {
        let input_channels = if Self::supports_config(sample_rate, channels) {
            channels
        } else if channels == 1 && Self::supports_config(sample_rate, 2) {
            warn!(
                "audio source requested mono input at {} Hz, but only stereo is supported; using channel 0",
                sample_rate
            );
            2
        } else {
            return Err(Error::InvalidParameter.into());
        };

        Ok(AudioSource {
            output: O::default(),
            output_channels: channels,
            sample_rate,
            channels: input_channels,
            stream: None,
            rx: None,
            buff: None,
        })
    }
}

#[doc(hidden)]
impl<O> Kernel for AudioSource<O>
where
    O: CpuBufferWriter<Item = f32>,
{
    async fn init(&mut self, _m: &mut MessageOutputs, _b: &mut BlockMeta) -> Result<()> {
        let device = cpal::default_host()
            .default_input_device()
            .expect("no input device available");

        let config = StreamConfig {
            channels: self.channels,
            sample_rate: self.sample_rate,
            buffer_size: BufferSize::Default,
        };

        let (tx, rx) = mpsc::unbounded();

        let stream = device.build_input_stream(
            &config,
            move |data, _| {
                let data = data.to_owned();
                tx.unbounded_send(data).unwrap();
            },
            move |err| {
                panic!("cpal stream error {err:?}");
            },
            None,
        )?;

        stream.play()?;

        self.rx = Some(rx);
        self.stream = Some(stream);

        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if let Some((buff, mut full)) = self.buff.take() {
            let o = self.output.slice();
            if self.output_channels == 1 && self.channels == 2 {
                let n = std::cmp::min(o.len(), (buff.len() - full) / 2);

                for j in 0..n {
                    o[j] = buff[full + 2 * j];
                }

                full += 2 * n;
                self.output.produce(n);
            } else {
                let n = std::cmp::min(o.len(), buff.len() - full);

                for (i, v) in o.iter_mut().take(n).enumerate() {
                    *v = buff[full + i]
                }

                full += n;
                self.output.produce(n);
            }

            if buff.len() == full {
                io.call_again = true;
                self.buff = None;
            } else {
                self.buff = Some((buff, full));
            }
        } else if let Some(v) = self.rx.as_mut().unwrap().next().await {
            io.call_again = true;
            self.buff = Some((v, 0));
        } else {
            io.finished = true;
        }

        Ok(())
    }
}
