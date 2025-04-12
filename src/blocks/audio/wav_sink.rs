use crate::prelude::*;
use std::path;

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
#[derive(Block)]
pub struct WavSink<T, I = circular::Reader<T>>
where
    T: Send + 'static + hound::Sample + Copy,
    I: CpuBufferReader<Item = T>,
{
    #[input]
    input: I,
    writer: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
}

impl<T, I> WavSink<T, I>
where
    T: Send + 'static + hound::Sample + Copy,
    I: CpuBufferReader<Item = T>,
{
    /// Create WAV Sink block
    pub fn new<P: AsRef<path::Path> + std::marker::Send + Copy>(
        file_name: P,
        spec: hound::WavSpec,
    ) -> Self {
        let writer = hound::WavWriter::create(file_name, spec).unwrap();
        Self {
            input: I::default(),
            writer,
        }
    }
}

#[doc(hidden)]
impl<T, I> Kernel for WavSink<T, I>
where
    T: Send + 'static + hound::Sample + Copy,
    I: CpuBufferReader<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let items = i.len();
        if items > 0 {
            for t in i {
                self.writer.write_sample(*t).unwrap();
            }
        }

        if self.input.finished() {
            io.finished = true;
        }

        self.input.consume(items);
        Ok(())
    }
}
