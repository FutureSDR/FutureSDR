use futuresdr::prelude::*;

/// In-place null source for `i32` streams.
#[derive(Block)]
pub struct NullSource<O = circuit::Writer<i32>>
where
    O: InplaceWriter<Item = i32>,
{
    #[output]
    output: O,
}

impl<O> NullSource<O>
where
    O: InplaceWriter<Item = i32>,
{
    /// Create [`NullSource`].
    pub fn new() -> Self {
        Self {
            output: O::default(),
        }
    }
}

impl<O> Default for NullSource<O>
where
    O: InplaceWriter<Item = i32>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<O> Kernel for NullSource<O>
where
    O: InplaceWriter<Item = i32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if let Some(mut b) = self.output.get_empty_buffer() {
            let n = b.slice().len();
            b.set_valid(n);
            self.output.put_full_buffer(b);
            if self.output.has_more_buffers() {
                io.call_again = true;
            }
        }
        Ok(())
    }
}

/// In-place head block for `i32` streams.
#[derive(Block)]
pub struct Head<I = circuit::Reader<i32>, O = circuit::Writer<i32>>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    remaining: u64,
    #[input]
    input: I,
    #[output]
    output: O,
}

impl<I, O> Head<I, O>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    /// Create [`Head`] that forwards at most `n_items`.
    pub fn new(n_items: u64) -> Self {
        Self {
            remaining: n_items,
            input: I::default(),
            output: O::default(),
        }
    }
}

impl<I, O> Kernel for Head<I, O>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if self.remaining == 0 {
            io.finished = true;
            return Ok(());
        }

        if let Some(mut b) = self.input.get_full_buffer() {
            let m = std::cmp::min(self.remaining as usize, b.slice().len());
            b.set_valid(m);
            self.remaining -= m as u64;
            self.output.put_full_buffer(b);

            if self.remaining == 0 {
                io.finished = true;
            } else if self.input.has_more_buffers() {
                io.call_again = true;
            } else if self.input.finished() {
                io.finished = true;
            }
        } else if self.input.finished() {
            io.finished = true;
        }

        Ok(())
    }
}

/// In-place add-1 block for `i32` streams.
#[derive(Block)]
pub struct Add<I = circuit::Reader<i32>, O = circuit::Writer<i32>>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    #[input]
    input: I,
    #[output]
    output: O,
}

impl<I, O> Add<I, O>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    /// Create [`Add`].
    pub fn new() -> Self {
        Self {
            input: I::default(),
            output: O::default(),
        }
    }
}

impl<I, O> Default for Add<I, O>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<I, O> Kernel for Add<I, O>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if let Some(mut b) = self.input.get_full_buffer() {
            b.slice().iter_mut().for_each(|x| *x = x.wrapping_add(1));
            self.output.put_full_buffer(b);

            if self.input.has_more_buffers() {
                io.call_again = true;
            } else if self.input.finished() {
                io.finished = true;
            }
        } else if self.input.finished() {
            io.finished = true;
        }
        Ok(())
    }
}

/// In-place null sink for `i32` streams.
#[derive(Block)]
pub struct NullSink<I = circuit::Reader<i32>>
where
    I: InplaceReader<Item = i32>,
{
    #[input]
    input: I,
    n_received: usize,
}

impl<I> NullSink<I>
where
    I: InplaceReader<Item = i32>,
{
    /// Create [`NullSink`].
    pub fn new() -> Self {
        Self {
            input: I::default(),
            n_received: 0,
        }
    }

    /// Total number of consumed samples.
    pub fn n_received(&self) -> usize {
        self.n_received
    }
}

impl<I> Default for NullSink<I>
where
    I: InplaceReader<Item = i32>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<I> Kernel for NullSink<I>
where
    I: InplaceReader<Item = i32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mio: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if let Some(mut b) = self.input.get_full_buffer() {
            self.n_received += b.slice().len();
            self.input.put_empty_buffer(b);

            if self.input.has_more_buffers() {
                io.call_again = true;
            } else if self.input.finished() {
                io.finished = true;
            }
        } else if self.input.finished() {
            io.finished = true;
        }
        Ok(())
    }
}
