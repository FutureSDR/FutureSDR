use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Reads chunks of size `WIDTH` and outputs an exponential moving average over a window of specified size.
///
/// # Example
/// See [`egui` example][egui] for example of using [`MovingAvg`] to
/// smooth over FFTs.
///
/// [egui]: https://github.com/FutureSDR/FutureSDR/blob/main/examples/egui/src/bin/combined.rs
#[derive(Block)]
pub struct MovingAvg<const WIDTH: usize> {
    decay_factor: f32,
    history_size: usize,
    i: usize,
    avg: [f32; WIDTH],
}

impl<const WIDTH: usize> MovingAvg<WIDTH> {
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
    pub fn new(decay_factor: f32, history_size: usize) -> TypedBlock<Self> {
        assert!(
            (0.0..=1.0).contains(&decay_factor),
            "decay_factor must be in [0, 1]"
        );
        TypedBlock::new(
            StreamIoBuilder::new()
                .add_input::<f32>("in")
                .add_output::<f32>("out")
                .build(),
            Self {
                decay_factor,
                history_size,
                i: 0,
                avg: [0.0; WIDTH],
            },
        )
    }
}

impl<const WIDTH: usize> Kernel for MovingAvg<WIDTH> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<f32>();
        let output = sio.output(0).slice::<f32>();

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

        if sio.input(0).finished() && consumed == input.len() / WIDTH {
            io.finished = true;
        }

        sio.input(0).consume(consumed * WIDTH);
        sio.output(0).produce(produced * WIDTH);

        Ok(())
    }
}
