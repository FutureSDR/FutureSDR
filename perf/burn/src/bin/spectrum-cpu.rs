#![recursion_limit = "512"]
use anyhow::Result;
use futuresdr::blocks::WebsocketSink;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;
use ndarray::Array;
use ndarray::Array1;
use ndarray::Array2;
use ndarray::Axis;
use ndarray::s;
use ndarray::stack;
use perf_burn::BATCH_SIZE;
use perf_burn::FFT_SIZE;

#[derive(Block)]
struct Fft {
    #[input]
    input: circuit::Reader<Complex32>,
    #[output]
    output: circuit::Writer<f32>,
    wr: Array2<f32>,
    wi: Array2<f32>,
}

impl Fft {
    fn new() -> Self {
        let k = Array::from_iter(0..FFT_SIZE as i64)
            .into_shape_clone((FFT_SIZE, 1))
            .expect("reshape failed");
        let n_idx = Array::from_iter(0..FFT_SIZE as i64)
            .into_shape_clone((1, FFT_SIZE))
            .expect("reshape failed");

        let k_f = k.mapv(|x| x as f32);
        let n_f = n_idx.mapv(|x| x as f32);
        let angle = k_f * n_f * (-2.0 * std::f32::consts::PI / FFT_SIZE as f32);

        let wr: Array2<f32> = angle.mapv(f32::cos);
        let wi: Array2<f32> = angle.mapv(f32::sin);

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
            && let Some(mut b) = self.input.get_full_buffer()
        {
            let t = Array::from_iter(b.slice().iter().flat_map(|c| [c.re, c.im]));
            assert_eq!(t.len(), BATCH_SIZE * FFT_SIZE * 2);
            let t = t.to_shape([BATCH_SIZE, FFT_SIZE, 2]).unwrap();

            let x_re_view = t.slice(s![.., .., 0]);
            let x_re = x_re_view
                .to_shape([BATCH_SIZE, FFT_SIZE])
                .unwrap()
                .reversed_axes();

            let x_im_view = t.slice(s![.., .., 1]);
            let x_im = x_im_view
                .to_shape([BATCH_SIZE, FFT_SIZE])
                .unwrap()
                .reversed_axes();

            let tmp = (self.wr.dot(&x_re) - self.wi.dot(&x_im)).reversed_axes();
            let x_im = (self.wr.dot(&x_im) + self.wi.dot(&x_re)).reversed_axes();
            let x_re = tmp;

            let power = x_re.mapv(|v| v.powi(2)) + x_im.mapv(|v| v.powi(2));
            let mag: Array1<f32> = power.mean_axis(Axis(0)).expect("non-empty axis");

            let half = FFT_SIZE / 2;
            let second = mag.slice(s![half..]).to_owned();
            let first = mag.slice(s![..half]).to_owned();
            let mag = stack(Axis(0), &[second.view(), first.view()]).expect("stack failed");

            self.input.put_empty_buffer(b);
            let mut b = self.output.get_empty_buffer().unwrap();
            b.slice().copy_from_slice(mag.as_slice().unwrap());
            self.output.put_full_buffer(b);

            if self.input.has_more_buffers() {
                io.call_again = true;
            }
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let mut fg = Flowgraph::new();

    let mut src = Builder::new("")?
        .frequency(100e6)
        .sample_rate(3.2e6)
        .gain(34.0)
        .build_source_with_buffer::<circuit::Writer<Complex32>>()?;
    src.outputs()[0].inject_buffers_with_items(4, BATCH_SIZE * FFT_SIZE);

    let mut fft = Fft::new();
    fft.output().inject_buffers_with_items(4, FFT_SIZE);

    let snk = WebsocketSink::<f32, circuit::Reader<f32>>::new(
        9001,
        WebsocketSinkMode::FixedBlocking(FFT_SIZE),
    );

    connect!(fg, src.outputs[0] > fft > snk);
    connect!(fg, src.outputs[0] < fft);
    connect!(fg, fft < snk);

    Runtime::new().run(fg)?;
    Ok(())
}
