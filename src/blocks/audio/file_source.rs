use rodio::source::Buffered;
use rodio::source::SamplesConverter;
use rodio::source::Source;
use rodio::Decoder;
use std::fs::File;
use std::io::BufReader;

use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Read an audio file and output its samples.
pub struct FileSource {
    src: Buffered<SamplesConverter<Decoder<BufReader<File>>, f32>>,
}

impl FileSource {
    /// Create FileSource block
    pub fn new(file: &str) -> TypedBlock<Self> {
        let file = BufReader::new(File::open(file).unwrap());
        let source = Decoder::new(file).unwrap();

        TypedBlock::new(
            BlockMetaBuilder::new("FileSource").build(),
            StreamIoBuilder::new().add_output::<f32>("out").build(),
            MessageIoBuilder::new().build(),
            FileSource {
                src: source.convert_samples().buffered(),
            },
        )
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
impl Kernel for FileSource {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = sio.output(0).slice::<f32>();

        let mut n = 0;
        for (i, v) in self.src.by_ref().take(out.len()).enumerate() {
            out[i] = v;
            n += 1;
        }

        sio.output(0).produce(n);
        if n < out.len() {
            io.finished = true;
        }

        Ok(())
    }
}
