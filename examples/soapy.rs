use anyhow::Result;

fn main() -> Result<()> {
    inner::main()
}

#[cfg(not(feature = "soapy"))]
mod inner {
    use anyhow::Result;

    pub(super) fn main() -> Result<()> {
        println!("Soapy feature not enabled.");
        Ok(())
    }
}

#[cfg(feature = "soapy")]
mod inner {

    use anyhow::Result;
    use async_trait::async_trait;
    use num_complex::Complex;
    use std::mem::size_of;

    use futuresdr::blocks::FftBuilder;
    use futuresdr::blocks::SoapySourceBuilder;
    use futuresdr::blocks::WebsocketSinkBuilder;
    use futuresdr::blocks::WebsocketSinkMode;
    use futuresdr::runtime::AsyncKernel;
    use futuresdr::runtime::Block;
    use futuresdr::runtime::BlockMeta;
    use futuresdr::runtime::BlockMetaBuilder;
    use futuresdr::runtime::Flowgraph;
    use futuresdr::runtime::MessageIo;
    use futuresdr::runtime::MessageIoBuilder;
    use futuresdr::runtime::Runtime;
    use futuresdr::runtime::StreamIo;
    use futuresdr::runtime::StreamIoBuilder;
    use futuresdr::runtime::WorkIo;

    pub(super) fn main() -> Result<()> {
        let mut fg = Flowgraph::new();

        let src = SoapySourceBuilder::new().build();
        let fft = FftBuilder::new().build();
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
        pub fn new() -> Block {
            Block::new_async(
                BlockMetaBuilder::new("ComplexToMag").build(),
                StreamIoBuilder::new()
                    .add_stream_input("in", size_of::<Complex<f32>>())
                    .add_stream_output("out", size_of::<f32>())
                    .build(),
                MessageIoBuilder::new().build(),
                ComplexToMag {},
            )
        }
    }

    #[async_trait]
    impl AsyncKernel for ComplexToMag {
        async fn work(
            &mut self,
            io: &mut WorkIo,
            sio: &mut StreamIo,
            _mio: &mut MessageIo<Self>,
            _meta: &mut BlockMeta,
        ) -> Result<()> {
            let i = sio.input(0).slice::<Complex<f32>>();
            let o = sio.output(0).slice::<f32>();

            let n = std::cmp::min(i.len(), o.len());

            for x in 0..n {
                let mut t = (((i[x].norm_sqr().log10() + 3.0) / 6.0 * 255.0) + 125.0) / 2.0;
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
}
