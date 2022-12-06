use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::blocks::TagDebug;
use futuresdr::blocks::VectorSource;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::Tag;
use futuresdr::runtime::WorkIo;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let n_items = 10000;
    let dummy_signal: Vec<f32> = vec![1.0, 2.0, 3.0]
        .into_iter()
        .cycle()
        .take(n_items)
        .collect();

    let src = fg.add_block(VectorSource::new(dummy_signal));
    // Add a tag every 5 samples
    let tagger = fg.add_block(PeriodicTagger::new(5));
    let snk = fg.add_block(TagDebug::<f32>::new("PeriodicTaggerDebugger"));

    fg.connect_stream(src, "out", tagger, "in")?;
    fg.connect_stream(tagger, "out", snk, "in")?;

    Runtime::new().run(fg)?;

    Ok(())
}

pub struct PeriodicTagger {
    period: usize,
}

impl PeriodicTagger {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(period: usize) -> Block {
        Block::new(
            BlockMetaBuilder::new("PeriodicTagger").build(),
            StreamIoBuilder::new()
                .add_input::<f32>("in")
                .add_output::<f32>("out")
                .build(),
            MessageIoBuilder::new().build(),
            Self { period },
        )
    }
}

#[async_trait]
impl Kernel for PeriodicTagger {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<f32>();
        let o = sio.output(0).slice::<f32>();

        let n = std::cmp::min(i.len(), o.len());
        for j in 0..n {
            o[j] = i[j];

            if j % self.period == 0 {
                // Tag output
                sio.output(0)
                    .add_tag(j, Tag::NamedUsize("my_tag".to_string(), j));
            }
        }
        sio.input(0).consume(n);
        sio.output(0).produce(n);

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
