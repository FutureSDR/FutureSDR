use rodio::source::Source;
use rodio::Decoder;
use std::fs::File;
use std::io::BufReader;

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

/// Read an audio file and output its samples.
pub struct FileSource {
    src: Box<dyn Source<Item = f32> + Send>,
}

impl FileSource {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(file: &str) -> Block {
        let file = BufReader::new(File::open(file).unwrap());
        let source = Decoder::new(file).unwrap();

        Block::new(
            BlockMetaBuilder::new("FileSource").build(),
            StreamIoBuilder::new().add_output::<f32>("out").build(),
            MessageIoBuilder::new().build(),
            FileSource {
                src: Box::new(source.convert_samples()),
            },
        )
    }

    pub fn sample_rate(&self) -> u32 {
        self.src.sample_rate()
    }

    pub fn channels(&self) -> u16 {
        self.src.channels()
    }
}

#[doc(hidden)]
#[async_trait]
impl Kernel for FileSource {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let out = sio.output(0).slice::<f32>();

        for (i, v) in self.src.by_ref().take(out.len()).enumerate() {
            out[i] = v;
        }
        sio.output(0).produce(out.len());

        Ok(())
    }
}
