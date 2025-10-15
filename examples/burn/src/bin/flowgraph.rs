#![recursion_limit = "512"]
use anyhow::Result;
use burn::backend::WebGpu;
use burn::prelude::*;
use futuresdr::prelude::burn_buffer::Buffer;
use futuresdr::prelude::*;
use inplace::VectorSink;
use inplace::VectorSource;

type B = WebGpu<f32, i32>;

#[derive(Block)]
struct Apply {
    #[input]
    input: burn_buffer::Reader<B, Int, i32>,
    #[output]
    output: burn_buffer::Writer<B, Int, i32>,
}

impl Apply {
    fn new() -> Self {
        Self {
            input: Default::default(),
            output: Default::default(),
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
            data.iter_mut().for_each(|i| *i += 1);
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
struct ApplyTensor<B>
where
    B: Backend,
{
    #[input]
    input: burn_buffer::Reader<B, Int, i32>,
    #[output]
    output: burn_buffer::Writer<B, Int, i32>,
}

impl<B> ApplyTensor<B>
where
    B: Backend,
{
    fn new() -> Self {
        Self {
            input: Default::default(),
            output: Default::default(),
        }
    }
}

impl<B> Default for ApplyTensor<B>
where
    B: Backend,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<B> Kernel for ApplyTensor<B>
where
    B: Backend,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        if let Some(b) = self.input.get_full_buffer() {
            let tensor = b.into_tensor();
            let tensor = tensor + 1;

            self.output.put_full_buffer(Buffer::from_tensor(tensor));
            self.input.notify_consumed_buffer();

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
    let device = burn::backend::wgpu::WgpuDevice::default();
    let mut fg = Flowgraph::new();

    let orig = Vec::from_iter(0..999_999i32);
    let mut src: VectorSource<i32, burn_buffer::Writer<B, Int, i32>> =
        VectorSource::new(orig.clone());
    src.output().set_device(&device);
    src.output().inject_buffers(4);
    let apply = Apply::new();
    let apply_tensor = ApplyTensor::new();
    let snk = VectorSink::new(orig.len());

    connect!(fg, src > apply > apply_tensor > snk);
    connect!(fg, src < apply_tensor);

    Runtime::new().run(fg)?;

    let snk = snk.get()?;
    assert_eq!(snk.items().len(), orig.len());
    snk.items()
        .iter()
        .zip(orig.iter())
        .for_each(|(a, b)| assert_eq!(*a, *b + 2));

    Ok(())
}
