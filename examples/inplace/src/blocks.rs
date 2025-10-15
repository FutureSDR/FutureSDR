use futuresdr::prelude::*;

#[derive(Block)]
pub struct VectorSource<T, O = circuit::Writer<T>>
where
    T: CpuSample,
    O: InplaceWriter<Item = T>,
{
    #[output]
    output: O,
    offset: usize,
    items: Vec<T>,
}

impl<T, O> VectorSource<T, O>
where
    T: CpuSample,
    O: InplaceWriter<Item = T>,
{
    pub fn new(items: Vec<T>) -> Self {
        Self {
            output: O::default(),
            offset: 0,
            items,
        }
    }
}

impl<T, O> Kernel for VectorSource<T, O>
where
    T: CpuSample,
    O: InplaceWriter<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        if let Some(mut b) = self.output.get_empty_buffer() {
            let data = b.slice();
            let m = std::cmp::min(data.len(), self.items.len() - self.offset);

            data[0..m].clone_from_slice(&self.items[self.offset..self.offset + m]);
            self.offset += m;
            b.set_valid(m);

            self.output.put_full_buffer(b);

            if self.offset == self.items.len() {
                io.finished = true;
            } else if self.output.has_more_buffers() {
                io.call_again = true;
            }
        }

        Ok(())
    }
}

#[derive(Block)]
pub struct Apply<I = circuit::Reader<i32>, O = circuit::Writer<i32>>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    #[input]
    input: I,
    #[output]
    output: O,
}

impl<I, O> Apply<I, O>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    pub fn new() -> Self {
        Self {
            input: I::default(),
            output: O::default(),
        }
    }
}

impl<I, O> Default for Apply<I, O>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<I, O> Kernel for Apply<I, O>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        if let Some(mut b) = self.input.get_full_buffer() {
            let data = b.slice();
            data.iter_mut().for_each(|i| *i = i.wrapping_add(1));
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

#[derive(Block)]
pub struct VectorSink<T, I = circuit::Reader<T>>
where
    T: CpuSample,
    I: InplaceReader<Item = T>,
{
    #[input]
    input: I,
    items: Vec<T>,
}

impl<T, I> VectorSink<T, I>
where
    T: CpuSample,
    I: InplaceReader<Item = T>,
{
    pub fn new(capacity: usize) -> Self {
        Self {
            input: I::default(),
            items: Vec::with_capacity(capacity),
        }
    }

    pub fn items(&self) -> &Vec<T> {
        &self.items
    }
}

impl<T, I> Kernel for VectorSink<T, I>
where
    T: CpuSample,
    I: InplaceReader<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        if let Some(mut b) = self.input.get_full_buffer() {
            self.items.extend_from_slice(b.slice());
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
