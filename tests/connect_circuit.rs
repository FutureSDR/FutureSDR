use anyhow::Result;
use futuresdr::async_io::block_on;
use futuresdr::prelude::*;

#[derive(Block)]
struct CircuitSource<O = circuit::Writer<i32>>
where
    O: InplaceWriter<Item = i32>,
{
    #[output]
    output: O,
    offset: usize,
    items: Vec<i32>,
    repeat: bool,
}

impl<O> CircuitSource<O>
where
    O: InplaceWriter<Item = i32>,
{
    fn new(items: Vec<i32>, repeat: bool) -> Self {
        Self {
            output: O::default(),
            offset: 0,
            items,
            repeat,
        }
    }
}

impl<O> Kernel for CircuitSource<O>
where
    O: InplaceWriter<Item = i32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if let Some(mut buffer) = self.output.get_empty_buffer() {
            let data = buffer.slice();
            let n = std::cmp::min(data.len(), self.items.len() - self.offset);

            data[..n].clone_from_slice(&self.items[self.offset..self.offset + n]);
            self.offset += n;
            buffer.set_valid(n);
            self.output.put_full_buffer(buffer);

            if self.offset == self.items.len() {
                if self.repeat {
                    self.offset = 0;
                } else {
                    io.finished = true;
                }
            } else if self.output.has_more_buffers() {
                io.call_again = true;
            }
        }

        Ok(())
    }
}

#[derive(Block)]
struct AddOne<I = circuit::Reader<i32>, O = circuit::Writer<i32>>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    #[input]
    input: I,
    #[output]
    output: O,
}

impl<I, O> AddOne<I, O>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    fn new() -> Self {
        Self {
            input: I::default(),
            output: O::default(),
        }
    }
}

impl<I, O> Kernel for AddOne<I, O>
where
    I: InplaceReader<Item = i32>,
    O: InplaceWriter<Item = i32, Buffer = I::Buffer>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if let Some(mut buffer) = self.input.get_full_buffer() {
            buffer
                .slice()
                .iter_mut()
                .for_each(|item| *item = item.wrapping_add(1));
            self.output.put_full_buffer(buffer);

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
struct CircuitSink<I = circuit::Reader<i32>>
where
    I: InplaceReader<Item = i32>,
{
    #[input]
    input: I,
    items: Vec<i32>,
}

impl<I> CircuitSink<I>
where
    I: InplaceReader<Item = i32>,
{
    fn new(capacity: usize) -> Self {
        Self {
            input: I::default(),
            items: Vec::with_capacity(capacity),
        }
    }

    fn items(&self) -> &[i32] {
        &self.items
    }
}

impl<I> Kernel for CircuitSink<I>
where
    I: InplaceReader<Item = i32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        if let Some(mut buffer) = self.input.get_full_buffer() {
            self.items.extend_from_slice(buffer.slice());
            self.input.put_empty_buffer(buffer);

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

#[test]
fn connect_circuit_executes() -> Result<()> {
    let input: Vec<i32> = (0..4096).collect();
    let expected: Vec<i32> = input.iter().map(|item| item + 1).collect();

    let mut fg = Flowgraph::new();
    let mut src: CircuitSource = CircuitSource::new(input, false);
    src.output().inject_buffers(4);
    let apply: AddOne = AddOne::new();
    let snk: CircuitSink = CircuitSink::new(expected.len());

    connect!(fg, src > apply > snk);
    connect!(fg, src < snk);

    let fg = Runtime::new().run(fg)?;
    let snk = snk.get(&fg)?;

    assert_eq!(snk.items(), expected);
    Ok(())
}

#[test]
fn connect_circuit_description_omits_closure_edge() -> Result<()> {
    let pattern = vec![3, 5, 8, 13, 21];

    let mut fg = Flowgraph::new();
    let mut src: CircuitSource = CircuitSource::new(pattern.clone(), true);
    src.output().inject_buffers(4);
    let apply: AddOne = AddOne::new();
    let snk: CircuitSink = CircuitSink::new(1024);

    connect!(fg, src > apply > snk);
    connect!(fg, src < snk);

    let expected_edges = vec![
        (
            src.id(),
            PortId::new("output"),
            apply.id(),
            PortId::new("input"),
        ),
        (
            apply.id(),
            PortId::new("output"),
            snk.id(),
            PortId::new("input"),
        ),
    ];

    let rt = Runtime::new();
    let (task, mut handle) = rt.start_sync(fg)?;
    let description = block_on(async {
        let description = handle.description().await?;
        handle.terminate_and_wait().await?;
        Ok::<_, Error>(description)
    })?;
    let fg = block_on(task)?;
    let snk = snk.get(&fg)?;

    assert_eq!(description.stream_edges, expected_edges);
    assert!(description.message_edges.is_empty());
    assert!(!snk.items().is_empty());
    for (index, item) in snk.items().iter().enumerate() {
        assert_eq!(*item, pattern[index % pattern.len()] + 1);
    }

    Ok(())
}
