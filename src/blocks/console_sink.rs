use crate::prelude::*;
use std::fmt::Debug;

/// Log stream data with [log::info!].
#[derive(Block)]
pub struct ConsoleSink<T, I = DefaultCpuReader<T>>
where
    T: Send + 'static + Debug,
    I: CpuBufferReader<Item = T>,
{
    #[input]
    input: I,
    sep: String,
}

impl<T, I> ConsoleSink<T, I>
where
    T: Send + 'static + Debug,
    I: CpuBufferReader<Item = T>,
{
    /// Create [`ConsoleSink`] block
    ///
    /// ## Parameter
    /// - `sep`: Separator between items
    pub fn new(sep: impl Into<String>) -> Self {
        Self {
            input: I::default(),
            sep: sep.into(),
        }
    }
}

#[doc(hidden)]
impl<T, I> Kernel for ConsoleSink<T, I>
where
    T: Send + 'static + Debug,
    I: CpuBufferReader<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = self.input.slice();
        let i_len = i.len();

        if !i.is_empty() {
            let s = i
                .iter()
                .map(|x| format!("{x:?}{}", &self.sep))
                .collect::<Vec<String>>()
                .concat();
            info!("{}", s);

            self.input.consume(i_len);
        }

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}
