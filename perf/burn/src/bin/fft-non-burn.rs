use anyhow::Result;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSource;
use futuresdr::runtime::dev::prelude::*;
use perf_burn::FFT_SIZE;
use perf_burn::TimedSink;
use perf_burn::batch_size_from_args;
use perf_burn::benchmark_input_samples;
use perf_burn::benchmark_output_items;

#[derive(Block)]
struct Avg {
    #[input]
    input: circular::Reader<Complex32>,
    #[output]
    output: circular::Writer<f32>,
    batch_size: usize,
}

impl Avg {
    fn new(batch_size: usize) -> Self {
        let mut input: circular::Reader<Complex32> = Default::default();
        input.set_min_items(FFT_SIZE * batch_size);
        let mut output: circular::Writer<f32> = Default::default();
        output.set_min_items(FFT_SIZE);

        Self {
            input,
            output,
            batch_size,
        }
    }
}

impl Kernel for Avg {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _b: &BlockMeta,
    ) -> Result<()> {
        let input = self.input.slice();
        let input_len = input.len();
        let output = self.output.slice();
        let output_len = output.len();

        if input_len >= FFT_SIZE * self.batch_size && output_len >= FFT_SIZE {
            for i in 0..FFT_SIZE {
                let mut sum = 0.0;
                for b in 0..self.batch_size {
                    sum += input[b * FFT_SIZE + i].norm_sqr();
                }
                output[i] = (sum / self.batch_size as f32 + 1.0e-30).log10();
            }

            self.input.consume(FFT_SIZE * self.batch_size);
            self.output.produce(FFT_SIZE);

            if input_len >= 2 * FFT_SIZE * self.batch_size && output_len >= 2 * FFT_SIZE {
                io.call_again = true;
            }
        }

        if self.input.finished() {
            let input = self.input.slice();
            if input.len() < FFT_SIZE * self.batch_size {
                io.finished = true;
            }
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    let batch_size = batch_size_from_args()?;
    futuresdr::runtime::init();
    futuresdr::runtime::config::set("buffer_size", (FFT_SIZE * batch_size * 8 * 2) as u64);

    let mut fg = Flowgraph::new();

    let src = NullSource::<Complex32>::new();
    let head = Head::<Complex32>::new(benchmark_input_samples(batch_size));
    let fft = Fft::with_options(FFT_SIZE, FftDirection::Forward, true, None);
    let avg = Avg::new(batch_size);
    let snk = TimedSink::<DefaultCpuReader<f32>>::new(FFT_SIZE, benchmark_output_items(batch_size));

    connect!(fg, src > head > fft > avg > snk);

    Runtime::new().run(fg)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futuresdr::blocks::VectorSink;
    use futuresdr::blocks::VectorSource;
    use perf_burn::test_utils::assert_spectrum_close;
    use perf_burn::test_utils::nontrivial_input;
    use perf_burn::test_utils::reference_spectrum;

    #[test]
    fn spectrum_matches_reference_for_nontrivial_input() -> Result<()> {
        // More than Fft's internal per-call limit exercises shutdown with queued input.
        let batch_size = 33;
        let input = nontrivial_input(batch_size);
        let expected = reference_spectrum(&input, batch_size);

        let mut fg = Flowgraph::new();
        let src = VectorSource::<Complex32>::new(input);
        let fft = Fft::with_options(FFT_SIZE, FftDirection::Forward, true, None);
        let avg = Avg::new(batch_size);
        let snk = VectorSink::<f32>::new(FFT_SIZE);
        connect!(fg, src > fft > avg > snk);

        let fg = Runtime::new().run(fg)?;
        let actual = fg.with(&snk, |snk| snk.items().clone())?;
        assert_spectrum_close(&actual, &expected);
        Ok(())
    }
}
