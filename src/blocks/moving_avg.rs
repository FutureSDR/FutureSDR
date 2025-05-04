use crate::prelude::*;

/// Reads chunks of size `WIDTH` and outputs an exponential moving average over a window of specified size.
///
/// # Example
/// See [`egui` example][egui] for example of using [`MovingAvg`] to
/// smooth over FFTs.
///
/// [egui]: https://github.com/FutureSDR/FutureSDR/blob/main/examples/egui/src/bin/combined.rs
#[derive(Block)]
pub struct MovingAvg<const WIDTH: usize, I = circular::Reader<f32>, O = circular::Writer<f32>>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = f32>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    decay_factor: f32,
    history_size: usize,
    i: usize,
    avg: [f32; WIDTH],
}

impl<const WIDTH: usize, I, O> MovingAvg<WIDTH, I, O>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = f32>,
{
    /// Instantiate moving average.
    ///
    /// # Arguments
    ///
    /// * `decay_factor`: amount current value should contribute to the rolling average.
    ///    Must be in `[0.0, 1.0]`.
    /// * `history_size`: number of chunks to average over
    ///
    /// Typical parameter values might be `decay_factor=0.1` and `history_size=3`
    ///
    /// # Panics
    /// Function will panic if `decay_factor` is not in `[0.0, 1.0]`
    pub fn new(decay_factor: f32, history_size: usize) -> Self {
        assert!(
            (0.0..=1.0).contains(&decay_factor),
            "decay_factor must be in [0, 1]"
        );
        Self {
            input: I::default(),
            output: O::default(),
            decay_factor,
            history_size,
            i: 0,
            avg: [0.0; WIDTH],
        }
    }
}

impl<const WIDTH: usize, I, O> Kernel for MovingAvg<WIDTH, I, O>
where
    I: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = f32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = self.input.slice();
        let output = self.output.slice();
        let input_len = input.len();

        let mut consumed = 0;
        let mut produced = 0;

        while (consumed + 1) * WIDTH <= input.len() && (produced + 1) * WIDTH <= output.len() {
            for i in 0..WIDTH {
                let t = input[consumed * WIDTH + i];
                if t.is_finite() {
                    self.avg[i] = (1.0 - self.decay_factor) * self.avg[i] + self.decay_factor * t;
                } else {
                    self.avg[i] *= 1.0 - self.decay_factor;
                }
            }

            self.i += 1;

            if self.i == self.history_size {
                output[produced * WIDTH..(produced + 1) * WIDTH].clone_from_slice(&self.avg);
                self.i = 0;
                produced += 1;
            }

            consumed += 1;
        }

        if self.input.finished() && consumed == input_len / WIDTH {
            io.finished = true;
        }

        self.input.consume(consumed * WIDTH);
        self.output.produce(produced * WIDTH);

        Ok(())
    }
}
