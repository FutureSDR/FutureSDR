use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;
use cpal::BufferSize;
use cpal::SampleRate;
use cpal::Stream;
use cpal::StreamConfig;
use futures::channel::mpsc;
use futures::StreamExt;

use crate::prelude::*;

/// Audio Source.
#[allow(clippy::type_complexity)]
#[derive(Block)]
pub struct AudioSource<O = circular::Writer<f32>>
where O: CpuBufferWriter<Item = f32>
{
    #[output]
    output: O,
    sample_rate: u32,
    channels: u16,
    stream: Option<Stream>,
    rx: Option<mpsc::UnboundedReceiver<Vec<f32>>>,
    buff: Option<(Vec<f32>, usize)>,
}

// cpal::Stream is !Send
#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl<O> Send for AudioSource<O> 
where O: CpuBufferWriter<Item = f32>
{}

impl<O> AudioSource<O>
where O: CpuBufferWriter<Item = f32>
{
    /// Create AudioSource block
    pub fn new(sample_rate: u32, channels: u16) -> Self {
            AudioSource {
                output: O,
                sample_rate,
                channels,
                stream: None,
                rx: None,
                buff: None,
            }
    }
}

#[doc(hidden)]
impl<O> Kernel for AudioSource<O>
where O: CpuBufferWriter<Item = f32>
{
    async fn init(
        &mut self,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .expect("no input device available");

        let config = StreamConfig {
            channels: self.channels,
            sample_rate: SampleRate(self.sample_rate),
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
            let n = std::cmp::min(o.len(), buff.len() - full);

            for (i, v) in o.iter_mut().take(n).enumerate() {
                *v = buff[full + i]
            }

            full += n;

            if buff.len() == full {
                io.call_again = true;
                self.buff = None;
            } else {
                self.buff = Some((buff, full));
            }

            self.output.produce(n);
        } else if let Some(v) = self.rx.as_mut().unwrap().next().await {
            io.call_again = true;
            self.buff = Some((v, 0));
        } else {
            io.finished = true;
        }

        Ok(())
    }
}
