use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::BufferSize;
use cpal::SampleRate;
use cpal::Stream;
use cpal::StreamConfig;

use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;
use futures::channel::mpsc;
use futures::StreamExt;

/// Audio Source.
#[allow(clippy::type_complexity)]
pub struct AudioSource {
    sample_rate: u32,
    channels: u16,
    stream: Option<Stream>,
    rx: Option<mpsc::UnboundedReceiver<Vec<f32>>>,
    buff: Option<(Vec<f32>, usize)>,
}

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for AudioSource {}

impl AudioSource {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: u32, channels: u16) -> Block {
        Block::new(
            BlockMetaBuilder::new("AudioSource").build(),
            StreamIoBuilder::new().add_output::<f32>("out").build(),
            MessageIoBuilder::new().build(),
            AudioSource {
                sample_rate,
                channels,
                stream: None,
                rx: None,
                buff: None,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for AudioSource {
    async fn init(
        &mut self,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .expect("no output device available");

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
                panic!("cpal stream error {:?}", err);
            },
        )?;

        stream.play()?;

        self.rx = Some(rx);
        self.stream = Some(stream);

        Ok(())
    }

    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if let Some((buff, mut full)) = self.buff.take() {
            let o = sio.output(0).slice::<f32>();
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

            sio.output(0).produce(n);
        } else if let Some(v) = self.rx.as_mut().unwrap().next().await {
            io.call_again = true;
            self.buff = Some((v, 0));
        } else {
            io.finished = true;
        }

        Ok(())
    }
}
