#![recursion_limit = "512"]
use anyhow::Result;
use burn::prelude::*;
use futuresdr::blocks::WebsocketSink;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::burn::Buffer;
use futuresdr_burn::BATCH_SIZE;
use futuresdr_burn::FFT_SIZE;

type B = burn::backend::Wgpu<f32, i32>;

#[derive(Block)]
struct Fft {
    #[input]
    input: burn_buffer::Reader<B, Float>,
    #[output]
    output: burn_buffer::Writer<B, Float>,
    wr: Tensor<B, 2>,
    wi: Tensor<B, 2>,
}

impl Fft {
    fn new(device: &Device<B>) -> Self {
        let k = Tensor::<B, 1, Int>::arange(0..FFT_SIZE as i64, device).reshape([FFT_SIZE, 1]);
        let n_idx = Tensor::<B, 1, Int>::arange(0..FFT_SIZE as i64, device).reshape([1, FFT_SIZE]);

        let angle = k
            .mul(n_idx)
            .float()
            .mul_scalar(-2.0 * std::f32::consts::PI / FFT_SIZE as f32);

        let wr = angle.clone().cos();
        let wi = angle.sin();

        Self {
            input: Default::default(),
            output: Default::default(),
            wr,
            wi,
        }
    }
}

impl Kernel for Fft {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        if self.output.has_more_buffers()
            && let Some(b) = self.input.get_full_buffer()
        {
            let t = b.into_tensor();
            let t = t.reshape([BATCH_SIZE, FFT_SIZE, 2]);

            let x_re = t
                .clone()
                .slice(s![.., .., 0])
                .reshape([BATCH_SIZE, FFT_SIZE]) // -> [batch, n]
                .transpose();

            let x_im = t
                .slice(s![.., .., 1])
                .reshape([BATCH_SIZE, FFT_SIZE]) // -> [batch, n]
                .transpose();

            let tmp = self
                .wr
                .clone()
                .matmul(x_re.clone())
                .sub(self.wi.clone().matmul(x_im.clone()))
                .transpose();
            let x_im = self
                .wr
                .clone()
                .matmul(x_im)
                .add(self.wi.clone().matmul(x_re))
                .transpose();
            let x_re = tmp;

            let mag = x_re
                .powi_scalar(2)
                .add(x_im.powi_scalar(2))
                // .sqrt()
                .mean_dim(0)
                .reshape([FFT_SIZE]);

            let half = FFT_SIZE / 2;
            let second_half = mag.clone().slice(0..half);
            let first_half = mag.slice(half..);
            let mag = Tensor::cat(vec![first_half, second_half], 0);

            let _ = self.output.get_empty_buffer().unwrap();
            self.output.put_full_buffer(Buffer::from_tensor(mag));
            self.input.notify_consumed_buffer();

            if self.input.has_more_buffers() {
                io.call_again = true;
            }
        }
        Ok(())
    }
}

#[derive(Block)]
struct Convert {
    #[input]
    input: circular::Reader<Complex32>,
    #[output]
    output: burn_buffer::Writer<B, Float>,
    current: Option<(Buffer<B, Float>, usize)>,
}

impl Convert {
    fn new() -> Self {
        Self {
            input: Default::default(),
            output: Default::default(),
            current: None,
        }
    }
}

impl Kernel for Convert {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        if self.current.is_none() {
            if let Some(mut b) = self.output.get_empty_buffer() {
                assert_eq!(b.num_host_elements(), BATCH_SIZE * FFT_SIZE * 2);
                // b.resize(BATCH_SIZE * FFT_SIZE * 2);
                b.set_valid(BATCH_SIZE * FFT_SIZE * 2);
                self.current = Some((b, 0));
            } else {
                return Ok(());
            }
        }

        let (buffer, offset) = self.current.as_mut().unwrap();
        let output = &mut buffer.slice()[*offset..];
        let input = self.input.slice();

        let m = std::cmp::min(input.len(), output.len() / 2);
        for i in 0..m {
            output[2 * i] = input[i].re;
            output[2 * i + 1] = input[i].im;
        }

        *offset += 2 * m;
        self.input.consume(m);

        if m == output.len() / 2 {
            let (b, _) = self.current.take().unwrap();
            self.output.put_full_buffer(b);
            if self.output.has_more_buffers() {
                io.call_again = true;
            }
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let device = Default::default();
    let mut fg = Flowgraph::new();

    let mut src = Builder::new("")?
        .frequency(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build_source()?;
    src.outputs()[0].set_min_buffer_size_in_items(1 << 15);

    let mut convert = Convert::new();
    convert.output().set_device(&device);
    convert
        .output()
        .inject_buffers_with_items(4, BATCH_SIZE * FFT_SIZE * 2);

    let mut fft = Fft::new(&device);
    fft.output().set_device(&device);
    fft.output().inject_buffers_with_items(4, FFT_SIZE);

    let snk = WebsocketSink::<f32, burn_buffer::Reader<B, Float>>::new(
        9001,
        WebsocketSinkMode::FixedBlocking(FFT_SIZE),
    );

    connect!(fg, src.outputs[0] > convert > fft > snk);
    connect!(fg, convert < fft);
    connect!(fg, fft < snk);

    Runtime::new().run(fg)?;
    Ok(())
}
