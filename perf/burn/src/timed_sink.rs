use futuresdr::runtime::dev::prelude::*;
use std::time::Instant;

/// Sink that excludes a fixed warm-up prefix before timing the remaining items.
#[derive(Block)]
pub struct TimedSink<I: CpuBufferReader<Item = f32> = DefaultCpuReader<f32>> {
    warmup_items: usize,
    expected_items: usize,
    measured_items: usize,
    start: Option<Instant>,
    #[input]
    input: I,
}

impl<I> TimedSink<I>
where
    I: CpuBufferReader<Item = f32>,
{
    pub fn new(warmup_items: usize, expected_items: usize) -> Self {
        Self {
            warmup_items,
            expected_items,
            measured_items: 0,
            start: None,
            input: I::default(),
        }
    }
}

impl<I> Kernel for TimedSink<I>
where
    I: CpuBufferReader<Item = f32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let available = self.input.slice().len();
        if available > 0 {
            let warmup = available.min(self.warmup_items);
            self.warmup_items -= warmup;

            if self.warmup_items == 0 && self.start.is_none() {
                self.start = Some(Instant::now());
            }

            self.measured_items += available - warmup;
            self.input.consume(available);
        }

        if self.input.finished() {
            if self.warmup_items != 0 {
                return Err(Error::RuntimeError(format!(
                    "timed sink received too few warm-up items: {} missing",
                    self.warmup_items
                ))
                .into());
            }
            if self.measured_items != self.expected_items {
                return Err(Error::RuntimeError(format!(
                    "timed sink received {} measured items, expected {}",
                    self.measured_items, self.expected_items
                ))
                .into());
            }

            let elapsed = self
                .start
                .ok_or_else(|| Error::RuntimeError("timed sink never started".to_string()))?
                .elapsed();
            println!("took {elapsed:?}");
            io.finished = true;
        }

        Ok(())
    }
}
