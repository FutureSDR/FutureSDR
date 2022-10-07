use crate::anyhow::Result;
use crate::runtime::Block;
use crate::runtime::BlockMeta;
use crate::runtime::BlockMetaBuilder;
use crate::runtime::Kernel;
use crate::runtime::MessageIo;
use crate::runtime::MessageIoBuilder;
use crate::runtime::StreamIo;
use crate::runtime::StreamIoBuilder;
use crate::runtime::WorkIo;

/// Drop samples, printing tags.
///
/// Console output is prefixed with the `name` to help differentiate the output from multiple tag debug blocks.
///
/// # Inputs
///
/// `in`: Stream to drop
///
/// # Outputs
///
/// No outputs
///
/// # Usage
/// ```
/// use futuresdr::blocks::TagDebug;
/// use futuresdr::runtime::Flowgraph;
/// use futuresdr::num_complex::Complex32;
///
/// let mut fg = Flowgraph::new();
///
/// let sink = fg.add_block(TagDebug::<Complex32>::new("foo"));
/// ```
pub struct TagDebug<T: Send + 'static> {
    name: String,
    n_received: usize,
    _type: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> TagDebug<T> {
    pub fn new(name: impl Into<String>) -> Block {
        Block::new(
            BlockMetaBuilder::new("TagDebug").build(),
            StreamIoBuilder::new().add_input::<T>("in").build(),
            MessageIoBuilder::new().build(),
            TagDebug::<T> {
                _type: std::marker::PhantomData,
                name: name.into(),
                n_received: 0,
            },
        )
    }
}

#[doc(hidden)]
#[async_trait]
impl<T: Send + 'static> Kernel for TagDebug<T> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<u8>();

        let n = i.len() / std::mem::size_of::<T>();
        sio.input(0)
            .tags()
            .iter()
            .filter(|x| x.index < n)
            .for_each(|x| {
                println!(
                    "TagDebug {}: buf {}/abs {} -- {:?}",
                    &self.name,
                    x.index,
                    self.n_received + x.index,
                    x.tag
                )
            });

        if n > 0 {
            sio.input(0).consume(n);
            self.n_received += n;
        }

        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
