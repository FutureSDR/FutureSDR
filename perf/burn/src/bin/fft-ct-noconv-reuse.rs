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
use burn_fusion::client::GlobalFusionClient;
use burn_fusion::stream::StreamId;
use burn_wgpu::WgpuDevice;
use bytemuck::cast_slice;
use cubecl::client::ComputeClient;
use cubecl_wgpu::WgpuServer;
use futuresdr::blocks::FileSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::burn::Buffer;
use futuresdr_burn::fft::bit_reversal_indices;
use futuresdr_burn::fft::fft_inplace;
use futuresdr_burn::fft::generate_stage_twiddles;
use perf_burn::BATCH_SIZE;
use perf_burn::FFT_SIZE;
use perf_burn::TimeIt;

pub type Cube = CubeBackend<WgpuRuntime, f32, i32, u32>;
pub type B = Fusion<Cube>;

#[derive(Block)]
struct Fft {
    #[input]
    input: burn_buffer::Reader<B, Float>,
    #[output]
    output: burn_buffer::Writer<B, Float>,
    rev: Tensor<B, 3, Int>,
    twiddles: Vec<Tensor<B, 4, Float>>,
    fft_shift: Tensor<B, 1, Int>,
    fusion_client: GlobalFusionClient<FusionCubeRuntime<WgpuRuntime, u32>>,
    cubecl_client: ComputeClient<WgpuServer>,
    wgpu_device_type: WgpuDevice,
}

impl Fft {
    fn new(device: &Device<B>) -> Self {
        let tmp = Tensor::<B, 1>::empty([1], device);
        let tmp = tmp.into_primitive().tensor();
        let wgpu_device_type = tmp.client.device().clone();
        let fusion_client = tmp.client.clone();
        let cube_tensor = fusion_client.resolve_tensor_float::<Cube>(tmp);
        let cubecl_client = cube_tensor.client;

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
            let allocation =
                self.cubecl_client
                    .create_tensor(byte_data, &[BATCH_SIZE * FFT_SIZE * 2], 4);

            let cube_tensor = CubeTensor::new(
                self.cubecl_client.clone(),
                allocation.handle,
                [BATCH_SIZE * FFT_SIZE * 2].into(),
                self.wgpu_device_type.clone(),
                allocation.strides,
                DType::F32,
            );

            let fusion_prim = self.fusion_client.register_tensor(
                cube_tensor.into(),
                vec![BATCH_SIZE * FFT_SIZE * 2].into(),
                StreamId::current(),
                DType::F32,
            );
            let primitive_enum = TensorPrimitive::Float(fusion_prim);
            let t = Tensor::<B, 1, Float>::from_primitive(primitive_enum);
            let t = t.reshape([BATCH_SIZE, FFT_SIZE, 2]);

            let t = fft_inplace(t, self.rev.clone(), &self.twiddles);

            let mag = t.powi_scalar(2).sum_dim(2).mean_dim(0).reshape([FFT_SIZE]);
            let shift = mag.gather(0, self.fft_shift.clone());

            let _ = self.output.get_empty_buffer().unwrap();
            self.output.put_full_buffer(Buffer::from_tensor(shift));
            self.input.put_empty_buffer(b);

            if self.input.has_more_buffers() {
                io.call_again = true;
            }
        }

        if self.input.finished() && !self.input.has_more_buffers() {
            io.finished = true;
        }

        Ok(())
    }
}

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let device = Default::default();
    let mut fg = Flowgraph::new();

    let mut src = FileSource::<Complex32, burn_buffer::Writer<B, Float, Complex32, f32>>::new(
        "data.cf32",
        false,
    );
    src.output().set_device(&device);
    src.output()
        .inject_buffers_with_items(4, BATCH_SIZE * FFT_SIZE * 2);

    let mut fft = Fft::new(&device);
    fft.output().set_device(&device);
    fft.output().inject_buffers_with_items(4, FFT_SIZE);

    let snk = TimeIt::new();

    connect!(fg, src > fft > snk);
    connect!(fg, src < fft);
    connect!(fg, fft < snk);

    Runtime::with_scheduler(futuresdr::runtime::scheduler::SmolScheduler::new(1, true)).run(fg)?;
    // Runtime::new().run(fg)?;
    Ok(())
}
