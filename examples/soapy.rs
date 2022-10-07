use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::blocks::Fft;
use futuresdr::blocks::SoapySourceBuilder;
use futuresdr::blocks::WebsocketSinkBuilder;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::num_complex::Complex32;
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
use futuresdr::runtime::WorkIo;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = SoapySourceBuilder::new()
        .freq(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build();
    let fft = Fft::new(2048);
    let mag = ComplexToMag::new();
    let snk = WebsocketSinkBuilder::<f32>::new(9001)
        .mode(WebsocketSinkMode::FixedDropping(2048))
        .build();

    let src = fg.add_block(src);
    let fft = fg.add_block(fft);
    let mag = fg.add_block(mag);
    let snk = fg.add_block(snk);

    fg.connect_stream(src, "out", fft, "in")?;
    fg.connect_stream(fft, "out", mag, "in")?;
    fg.connect_stream(mag, "out", snk, "in")?;

    Runtime::new().run(fg)?;
    Ok(())
}

pub struct ComplexToMag {}

impl ComplexToMag {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("ComplexToMag").build(),
            StreamIoBuilder::new()
                .add_input::<Complex32>("in")
                .add_output::<Complex32>("out")
                .build(),
            MessageIoBuilder::new().build(),
            Self {},
        )
    }
}

#[async_trait]
impl Kernel for ComplexToMag {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<Complex32>();
        let o = sio.output(0).slice::<f32>();

        let n = std::cmp::min(i.len(), o.len());

        for x in 0..n {
            let mut t = ((i[x].norm_sqr().log10() + 3.0) / 6.0).mul_add(255.0, 125.0) / 2.0;
            t = t.clamp(0.0, 255.0);
            o[x] = t;
        }

        if sio.input(0).finished() && n == i.len() {
            io.finished = true;
        }

        if n == 0 {
            return Ok(());
        }

        sio.input(0).consume(n);
        sio.output(0).produce(n);

        Ok(())
    }
}
