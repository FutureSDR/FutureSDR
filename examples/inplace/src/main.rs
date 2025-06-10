use anyhow::Result;
use futuresdr::prelude::*;

#[derive(Block)]
struct VectorSource<T>
where
    T: CpuSample,
{
    #[output]
    output: circuit::Writer<T>,
    offset: usize,
    items: Vec<T>,
}

impl<T> VectorSource<T>
where
    T: CpuSample,
{
    fn new(items: Vec<T>) -> Self {
        Self {
            output: circuit::Writer::new(),
            offset: 0,
            items,
        }
    }
}

impl<T> Kernel for VectorSource<T>
where
    T: CpuSample,
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
struct Apply {
    #[input]
    input: circuit::Reader<u32>,
    #[output]
    output: circuit::Writer<u32>,
}

impl Apply {
    fn new() -> Self {
        Self {
            input: circuit::Reader::<u32>::new(),
            output: circuit::Writer::<u32>::new(),
        }
    }
}

impl Kernel for Apply {
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
struct VectorSink<T>
where
    T: CpuSample,
{
    #[input]
    input: circuit::Reader<T>,
    items: Vec<T>,
}

impl<T> VectorSink<T>
where
    T: CpuSample,
{
    fn new(capacity: usize) -> Self {
        Self {
            input: circuit::Reader::default(),
            items: Vec::with_capacity(capacity),
        }
    }
}

impl<T> Kernel for VectorSink<T>
where
    T: CpuSample,
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

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();

    let orig = Vec::from_iter(0..999_999u32);

    let mut src = VectorSource::new(orig.clone());
    src.output().inject_buffers(4);
    let apply = Apply::new();
    let snk = VectorSink::new(orig.len());

    connect!(fg, src > apply > snk);
    connect!(fg, src < snk);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
    assert_eq!(snk.items.len(), orig.len());
    snk.items
        .iter()
        .zip(orig.iter())
        .for_each(|(a, b)| assert_eq!(*a, b.wrapping_add(1)));

    Ok(())
}
