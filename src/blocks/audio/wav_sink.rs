use hound;
use std::path;

use crate::anyhow::Result;
use crate::async_trait::async_trait;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

/// Write samples to a WAV file.
///
/// # Usage
/// ```
/// use futuresdr::blocks::Apply;
/// use futuresdr::blocks::audio::WavSink;
/// use futuresdr::blocks::VectorSource;
/// use futuresdr::runtime::Flowgraph;
/// use futuresdr::runtime::Runtime;
/// use std::path::Path;
///
/// let filename = "/tmp/output.wav";
/// let path = Path::new(filename);
/// let spec = hound::WavSpec {
///     channels: 1,
///     sample_rate: 48_000,
///     bits_per_sample: 32,
///     sample_format: hound::SampleFormat::Float,
/// };
/// let mut fg = Flowgraph::new();
/// let src = fg.add_block(VectorSource::<f32>::new(vec![1.45, 2.4, 3.14, 4.2]));
/// let snk = fg.add_block(WavSink::<f32>::new(path, spec));
/// Runtime::new().run(fg);
/// ```
pub struct WavSink<T>
where
    T: Send + 'static + hound::Sample + Copy,
{
    writer: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static + hound::Sample + Copy> WavSink<T> {
    pub fn new<P: AsRef<path::Path> + std::marker::Send + Copy>(
        file_name: P,
        spec: hound::WavSpec,
    ) -> Block {
        let writer = hound::WavWriter::create(file_name, spec).unwrap();
        Block::new(
            BlockMetaBuilder::new("WavSink").build(),
            StreamIoBuilder::new().add_input::<T>("in").build(),
            MessageIoBuilder::new().build(),
            WavSink::<T> {
                writer,
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static + hound::Sample + Copy> Kernel for WavSink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();
        let items = i.len();
        if items > 0 {
            for t in i {
                self.writer.write_sample(*t).unwrap();
            }
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        sio.input(0).consume(items);
        Ok(())
    }
}
