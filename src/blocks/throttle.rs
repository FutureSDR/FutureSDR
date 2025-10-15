use crate::prelude::*;
use web_time::Instant;

/// Limit sample rate.
///
/// # Inputs
///
/// `in`: Input
///
/// # Outputs
///
/// `out`: Output
///
/// # Usage
/// ```
/// use futuresdr::blocks::Throttle;
///
/// let throttle = Throttle::<u8>::new(1_000_000.0);
/// ```
#[derive(Block)]
pub struct Throttle<
    T: Copy + Send + 'static,
    I: CpuBufferReader<Item = T> = DefaultCpuReader<T>,
    O: CpuBufferWriter<Item = T> = DefaultCpuWriter<T>,
> {
    #[input]
    input: I,
    #[output]
    output: O,
    rate: f64,
    t_init: Instant,
    n_items: usize,
}

impl<T, I, O> Throttle<T, I, O>
where
    T: Copy + Send + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    /// Creates a new Throttle block which will throttle to the specified rate.
    pub fn new(rate: f64) -> Self {
        Self {
            input: I::default(),
            output: O::default(),
            rate,
            t_init: Instant::now(),
            n_items: 0,
        }
    }
}

#[doc(hidden)]
impl<T, I, O> Kernel for Throttle<T, I, O>
where
    T: Copy + Send + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let o = self.output.slice();
        let i_len = i.len();

        let now = Instant::now();
        let target_items = (now - self.t_init).as_secs_f64() * self.rate;
        let target_items = target_items.floor() as usize;
        let remaining_items = target_items - self.n_items;

        let m = *[remaining_items, i.len(), o.len()]
            .iter()
            .min()
            .unwrap_or(&0);

        if m != 0 {
            o[..m].copy_from_slice(&i[..m]);
            self.n_items += m;
            self.input.consume(m);
            self.output.produce(m);
        }

        if self.input.finished() && i_len == m {
            io.finished = true;
        }

        io.block_on(async {
            #[cfg(target_arch = "wasm32")]
            gloo_timers::future::TimeoutFuture::new(100).await;
            #[cfg(not(target_arch = "wasm32"))]
            async_io::Timer::after(std::time::Duration::from_millis(100)).await;
        });

        Ok(())
    }

    async fn init(&mut self, _mio: &mut MessageOutputs, _meta: &mut BlockMeta) -> Result<()> {
        self.t_init = Instant::now();
        self.n_items = 0;
        Ok(())
    }
}
