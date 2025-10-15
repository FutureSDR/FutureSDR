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
use futuresdr_burn::fft::bit_reversal_indices;
use futuresdr_burn::fft::fft_inplace;
use futuresdr_burn::fft::generate_stage_twiddles;

type B = burn::backend::Wgpu<f32, i32>;

#[derive(Block)]
struct Fft {
    #[input]
    input: burn_buffer::Reader<B, Float>,
    #[output]
    output: burn_buffer::Writer<B, Float>,
    rev: Tensor<B, 3, Int>,
    twiddles: Vec<Tensor<B, 4, Float>>,
    fft_shift: Tensor<B, 1, Int>,
}

impl Fft {
    fn new(device: &Device<B>) -> Self {
        let rev = bit_reversal_indices(11);
        let rev = Tensor::<B, 1, Int>::from_ints(
            TensorData::new(
                rev.iter().map(|&i| i as i32).collect::<Vec<i32>>(),
                [FFT_SIZE],
            ),
            device,
        )
        .reshape([1, FFT_SIZE, 1])
        .repeat_dim(0, BATCH_SIZE)
        .repeat_dim(2, 2); // → [batch,n,1]

        let mut twiddles = Vec::new();
        twiddles.push(Tensor::empty([0, 0, 0, 0], device));
        for s in 1..=11 {
            let m = 1 << s;
            let half = m >> 1;
            let twiddle = generate_stage_twiddles(s, device).reshape([1, 1, half, 2]);
            twiddles.push(twiddle);
        }

        let fft_shift = Tensor::from_data(
            TensorData::new((1024..2048).chain(0..1024).collect(), [FFT_SIZE]),
            device,
        );

        Self {
            input: Default::default(),
            output: Default::default(),
            rev,
            twiddles,
            fft_shift,
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
            let t = fft_inplace(t, self.rev.clone(), &self.twiddles);

            let mag = t.powi_scalar(2).sum_dim(2).mean_dim(0).reshape([FFT_SIZE]);
            let shift = mag.gather(0, self.fft_shift.clone());

            let _ = self.output.get_empty_buffer().unwrap();
            self.output.put_full_buffer(Buffer::from_tensor(shift));
            self.input.notify_consumed_buffer();

            if self.input.has_more_buffers() {
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
        .build_source_with_buffer::<burn_buffer::Writer<B, Float, Complex32, f32>>()?;
    src.outputs()[0].set_device(&device);
    src.outputs()[0].inject_buffers_with_items(32, BATCH_SIZE * FFT_SIZE * 2);

    let mut fft = Fft::new(&device);
    fft.output().set_device(&device);
    fft.output().inject_buffers_with_items(8, FFT_SIZE);

    let snk = WebsocketSink::<f32, burn_buffer::Reader<B>>::new(
        9001,
        WebsocketSinkMode::FixedBlocking(FFT_SIZE),
    );

    connect!(fg, src.outputs[0] > fft > snk);
    connect!(fg, src.outputs[0] < fft);
    connect!(fg, fft < snk);

    Runtime::new().run(fg)?;
    Ok(())
}
