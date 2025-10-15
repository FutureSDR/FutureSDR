#![recursion_limit = "512"]
use anyhow::Result;
use burn::backend::wgpu::WgpuRuntime;
use burn::prelude::*;
use burn::tensor::DType;
use burn::tensor::TensorPrimitive;
use burn_cubecl::CubeBackend;
use burn_cubecl::fusion::FusionCubeRuntime;
use burn_cubecl::tensor::CubeTensor;
use burn_fusion::Fusion;
use burn_fusion::client::FusionClient;
use burn_fusion::client::MutexFusionClient;
use burn_fusion::stream::StreamId;
use burn_wgpu::WgpuDevice;
use bytemuck::cast_slice;
use cubecl::channel::MutexComputeChannel;
use cubecl::client::ComputeClient;
use cubecl_wgpu::WgpuServer;
use futuresdr::blocks::WebsocketSink;
use futuresdr::blocks::WebsocketSinkMode;
use futuresdr::blocks::seify::Builder;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::burn::Buffer;
use futuresdr_burn::BATCH_SIZE;
use futuresdr_burn::FFT_SIZE;

pub type Cube = CubeBackend<WgpuRuntime, f32, i32, u32>;
pub type B = Fusion<Cube>;

#[derive(Block)]
struct Fft {
    #[input]
    input: burn_buffer::Reader<B>,
    #[output]
    output: burn_buffer::Writer<B>,
    wr: Tensor<B, 2>,
    wi: Tensor<B, 2>,
    fusion_client: MutexFusionClient<FusionCubeRuntime<WgpuRuntime, u32>>,
    cubecl_client: ComputeClient<WgpuServer, MutexComputeChannel<WgpuServer>>,
    wgpu_device_type: WgpuDevice,
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

        let tmp = Tensor::<B, 1>::empty([1], device);
        let tmp = tmp.into_primitive().tensor();
        let wgpu_device_type = tmp.client.device().clone();
        let fusion_client = tmp.client.clone();
        let cube_tensor = fusion_client.resolve_tensor_float::<Cube>(tmp);
        let cubecl_client = cube_tensor.client;

        Self {
            input: Default::default(),
            output: Default::default(),
            wr,
            wi,
            fusion_client,
            cubecl_client,
            wgpu_device_type,
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
            let data = b.slice();
            let byte_data: &[u8] = cast_slice(data);
            let (handle, strides) =
                self.cubecl_client
                    .create_tensor(byte_data, &[BATCH_SIZE * FFT_SIZE * 2], 4);

            let cube_tensor = CubeTensor::new(
                self.cubecl_client.clone(),
                handle,
                [BATCH_SIZE * FFT_SIZE * 2].into(),
                self.wgpu_device_type.clone(),
                strides,
                DType::F32,
            );

            let fusion_prim = self.fusion_client.register_tensor(
                cube_tensor.into(),
                vec![BATCH_SIZE * FFT_SIZE * 2],
                StreamId::current(),
                DType::F32,
            );
            let primitive_enum = TensorPrimitive::Float(fusion_prim);
            let t = Tensor::<B, 1, Float>::from_primitive(primitive_enum);

            assert_eq!(t.shape().num_elements(), BATCH_SIZE * FFT_SIZE * 2);
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
            self.input.put_empty_buffer(b);

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
    output: burn_buffer::Writer<B>,
    current: Option<(Buffer<B>, usize)>,
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

    let snk = WebsocketSink::<f32, burn_buffer::Reader<B>>::new(
        9001,
        WebsocketSinkMode::FixedBlocking(FFT_SIZE),
    );

    connect!(fg, src.outputs[0] > convert > fft > snk);
    connect!(fg, convert < fft);
    connect!(fg, fft < snk);

    Runtime::new().run(fg)?;
    Ok(())
}
