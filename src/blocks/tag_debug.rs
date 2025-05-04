use crate::prelude::*;

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
#[derive(Block)]
pub struct TagDebug<T, I = circular::Reader<T>>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    #[input]
    input: I,
    name: String,
    n_received: usize,
}

impl<T, I> TagDebug<T, I>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    /// Create Tag Debug block
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            input: I::default(),
            name: name.into(),
            n_received: 0,
        }
    }
}

#[doc(hidden)]
impl<T, I> Kernel for TagDebug<T, I>
where
    T: Send + 'static,
    I: CpuBufferReader<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let (i, tags) = self.input.slice_with_tags();
        let n = i.len();

        tags.iter().filter(|x| x.index < n).for_each(|x| {
            println!(
                "TagDebug {}: buf {}/abs {} -- {:?}",
                &self.name,
                x.index,
                self.n_received + x.index,
                x.tag
            )
        });

        self.input.consume(n);
        self.n_received += n;

        if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}
