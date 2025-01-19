use crate::runtime::BlockMeta;
use crate::runtime::Kernel;
use crate::runtime::MessageOutputs;
use crate::runtime::Result;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::TypedBlock;
use crate::runtime::WorkIo;

/// Log stream data with [log::info!].
#[derive(Block)]
pub struct ConsoleSink<T: Send + 'static + std::fmt::Debug> {
    sep: String,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static + std::fmt::Debug> ConsoleSink<T> {
    /// Create [`ConsoleSink`] block
    ///
    /// ## Parameter
    /// - `sep`: Separator between items
    pub fn new(sep: impl Into<String>) -> TypedBlock<Self> {
        TypedBlock::new(
            StreamIoBuilder::new().add_input::<T>("in").build(),
            Self {
                sep: sep.into(),
                _type: std::marker::PhantomData,
            },
        )
    }
}

#[doc(hidden)]
impl<T: Send + 'static + std::fmt::Debug> Kernel for ConsoleSink<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();

        if !i.is_empty() {
            let s = i
                .iter()
                .map(|x| format!("{x:?}{}", &self.sep))
                .collect::<Vec<String>>()
                .concat();
            info!("{}", s);

            sio.input(0).consume(i.len());
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
