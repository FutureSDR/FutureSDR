use rodio::source::Buffered;
use rodio::source::SamplesConverter;
use rodio::source::Source;
use rodio::Decoder;
use std::fs::File;
use std::io::BufReader;

use crate::prelude::*;

/// Read an audio file and output its samples.
#[derive(Block)]
pub struct FileSource<O = circular::Writer<f32>>
where O: CpuBufferWriter<Item = f32>
{
    #[output]
    output: O,
    src: Buffered<SamplesConverter<Decoder<BufReader<File>>, f32>>,
}

impl<O> FileSource<O>
where O: CpuBufferWriter<Item = f32>
{
    /// Create FileSource block
    pub fn new(file: &str) -> Self {
        let file = BufReader::new(File::open(file).unwrap());
        let source = Decoder::new(file).unwrap();

            FileSource {
                output: O::default(),
                src: source.convert_samples().buffered(),
            }
    }
    /// Get sample rate
    pub fn sample_rate(&self) -> u32 {
        self.src.sample_rate()
    }
    /// Get number of samples
    pub fn channels(&self) -> u16 {
        self.src.channels()
    }
}

#[doc(hidden)]
impl<O> Kernel for FileSource<O>
where O: CpuBufferWriter<Item = f32>
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = self.output.slice();
        let o_len = out.len();

        let mut n = 0;
        for (i, v) in self.src.by_ref().take(out.len()).enumerate() {
            out[i] = v;
            n += 1;
        }

        self.output.produce(n);
        if n < o_len {
            io.finished = true;
        }

        Ok(())
    }
}
