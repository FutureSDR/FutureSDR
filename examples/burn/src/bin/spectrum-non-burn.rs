#![recursion_limit = "512"]
use anyhow::Result;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::WebsocketSink;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;
use futuresdr_burn::BATCH_SIZE;
use futuresdr_burn::FFT_SIZE;

#[derive(Block)]
struct Avg {
    #[input]
    input: circular::Reader<Complex32>,
    #[output]
    output: circular::Writer<f32>,
}

impl Avg {
    fn new() -> Self {
        let mut input: circular::Reader<Complex32> = Default::default();
        input.set_min_items(FFT_SIZE * BATCH_SIZE);
        let mut output: circular::Writer<f32> = Default::default();
        output.set_min_items(FFT_SIZE);

        Self { input, output }
    }
}

impl Kernel for Avg {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = self.input.slice();
        let input_len = input.len();
        let output = self.output.slice();
        let output_len = output.len();

        if input_len >= FFT_SIZE * BATCH_SIZE && output_len >= FFT_SIZE {
            for i in 0..FFT_SIZE {
                let mut sum = 0.0;
                for b in 0..BATCH_SIZE {
                    sum += input[b * FFT_SIZE + i].norm_sqr();
                }
                output[i] = sum / BATCH_SIZE as f32;
            }

            self.input.consume(FFT_SIZE * BATCH_SIZE);
            self.output.produce(FFT_SIZE);

            if input_len >= 2 * FFT_SIZE * BATCH_SIZE && output_len >= 2 * FFT_SIZE {
                io.call_again = true;
            }
        }

        if self.input.finished() {
            let input = self.input.slice();
            if input.len() < FFT_SIZE * BATCH_SIZE {
                io.finished = true;
            }
        }

        Ok(())
    }
}
fn main() -> Result<()> {
    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    let mut src = Builder::new("")?
        .frequency(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build_source()?;
    src.outputs()[0].set_min_buffer_size_in_items(1 << 15);

    let fft: Fft = Fft::with_options(FFT_SIZE, FftDirection::Forward, true, None);
    let avg = Avg::new();
    let snk = WebsocketSink::<f32>::new(9001, WebsocketSinkMode::FixedBlocking(FFT_SIZE));

    connect!(fg, src.outputs[0] > fft > avg > snk);

    Runtime::new().run(fg)?;
    Ok(())
}
