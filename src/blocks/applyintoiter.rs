use futuresdr::prelude::*;

/// Apply a function on each input sample to create an iterator and output its values.
#[derive(Block)]
pub struct ApplyIntoIter<
    F,
    A,
    B,
    I = circular::Reader<A>,
    O = circular::Writer<<B as IntoIterator>::Item>,
> where
    F: FnMut(&A) -> B + Send + 'static,
    A: Send + Sync + 'static,
    B: Send + 'static + IntoIterator,
    <B as IntoIterator>::Item: Send + Sync + 'static,
    I: CpuBufferReader<Item = A>,
    O: CpuBufferWriter<Item = B::Item>,
{
    f: F,
    current_it: Box<dyn Iterator<Item = B::Item> + Send>,
    #[input]
    input: I,
    #[output]
    output: O,
}

impl<F, A, B, I, O> ApplyIntoIter<F, A, B, I, O>
where
    F: FnMut(&A) -> B + Send + 'static,
    A: Send + Sync + 'static,
    B: Send + 'static + IntoIterator,
    <B as IntoIterator>::Item: Send + Sync + 'static,
    <B as IntoIterator>::IntoIter: Send,
    I: CpuBufferReader<Item = A>,
    O: CpuBufferWriter<Item = B::Item>,
{
    /// Create [`ApplyIntoIter`] block
    ///
    /// ## Parameter
    /// - `f`: Function to create an interator from an input sample
    pub fn new(f: F) -> Self {
        Self {
            f,
            current_it: Box::new(std::iter::empty()),
            input: I::default(),
            output: O::default(),
        }
    }
}

#[doc(hidden)]
impl<F, A, B, I, O> Kernel for ApplyIntoIter<F, A, B, I, O>
where
    F: FnMut(&A) -> B + Send + 'static,
    A: Send + Sync + 'static,
    B: Send + Sync + 'static + IntoIterator,
    <B as IntoIterator>::Item: Send + Sync + 'static,
    <B as IntoIterator>::IntoIter: Send,
    I: CpuBufferReader<Item = A>,
    O: CpuBufferWriter<Item = B::Item>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let (i, tags) = self.input.slice_with_tags();
        let (o, mut o_tags) = self.output.slice_with_tags();
        let i_len = i.len();
        let o_len = o.len();
        let mut i_iter = i.iter();

        let mut consumed = 0;
        let mut produced = 0;

        while produced < o_len {
            if let Some(v) = self.current_it.next() {
                o[produced] = v;
                produced += 1;
            } else if let Some(v) = i_iter.next() {
                self.current_it = Box::new(((self.f)(v)).into_iter());
                if let Some(ItemTag { tag, .. }) =
                    tags.iter().find(|x| x.index == consumed).cloned()
                {
                    o_tags.add_tag(produced, tag);
                }
                consumed += 1;
            } else {
                break;
            }
        }

        self.input.consume(consumed);
        self.output.produce(produced);
        if self.input.finished() && consumed == i_len && produced < o_len {
            io.finished = true;
        }

        Ok(())
    }
}
