use async_io::block_on;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::BufferSize;
use cpal::SampleRate;
use cpal::Stream;
use cpal::StreamConfig;

use crate::anyhow::Result;
use crate::runtime::AsyncKernel;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::SinkExt;
use futures::StreamExt;

#[allow(clippy::type_complexity)]
pub struct AudioSink {
    sample_rate: u32,
    channels: u16,
    stream: Option<Stream>,
    rx: Option<mpsc::UnboundedReceiver<(usize, oneshot::Sender<Box<[f32]>>)>>,
    buff: Option<(Box<[f32]>, usize, oneshot::Sender<Box<[f32]>>)>,
}

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl Send for AudioSink {}

impl AudioSink {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: u32, channels: u16) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("AudioSink").build(),
            StreamIoBuilder::new().add_input("in", 4).build(),
            MessageIoBuilder::new().build(),
            AudioSink {
                sample_rate,
                channels,
                stream: None,
                rx: None,
                buff: None,
            },
        )
    }
}

#[async_trait]
impl AsyncKernel for AudioSink {
    async fn init(
        &mut self,
        _s: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .expect("no output device available");

        let config = StreamConfig {
            channels: self.channels,
            sample_rate: SampleRate(self.sample_rate),
            buffer_size: BufferSize::Default,
        };

        let (mut tx, rx) = mpsc::unbounded();

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let (my_tx, my_rx) = oneshot::channel::<Box<[f32]>>();
                    let samples = block_on(async {
                        tx.send((data.len(), my_tx)).await.unwrap();
                        my_rx.await.unwrap()
                    });
                    assert_eq!(data.len(), samples.len());
                    for (i, s) in samples.iter().enumerate() {
                        data[i] = *s;
                    }
                },
                move |err| {
                    panic!("cpal stream error {:?}", err);
                },
            )
            .expect("could not build output stream");
        // On Windows there is an issue in cpal with
        // shared devices, if the requested configuration
        // does not match the device configuration.
        // https://github.com/RustAudio/cpal/issues/593

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
        if let Some((mut buff, mut full, tx)) = self.buff.take() {
            let i = sio.input(0).slice::<f32>();
            let n = std::cmp::min(i.len(), buff.len() - full);

            for (i, s) in i.iter().take(n).enumerate() {
                buff[full + i] = *s;
            }

            full += n;

            if buff.len() == full {
                tx.send(buff).unwrap();
                self.buff = None;
            } else {
                self.buff = Some((buff, full, tx));
            }

            sio.input(0).consume(n);
        } else if let Some((n, tx)) = self.rx.as_mut().unwrap().next().await {
            io.call_again = true;
            self.buff = Some((vec![0f32; n].into_boxed_slice(), 0, tx));
        } else {
            io.finished = true;
        }

        Ok(())
    }
}
