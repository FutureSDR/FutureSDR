#![recursion_limit = "512"]
use anyhow::Result;
use burn::backend::WebGpu;
use burn::prelude::*;
use burn::record::CompactRecorder;
use burn::record::Recorder;
use futuresdr_burn::TrainingConfig;
use ndarray::Array1;
use ndarray::Array3;

pub fn infer<B: Backend>(
    artifact_dir: &str,
    device: B::Device,
    samples: Tensor<B, 3>,
    snrs: Array1<f32>,
    mods: Array1<u8>,
) {
    let config = TrainingConfig::load(format!("{artifact_dir}/config.json"))
        .expect("Config should exist for the model; run train first");
    let record = CompactRecorder::new()
        .load(format!("{artifact_dir}/model").into(), &device)
        .expect("Trained model should exist; run train first");
    let model = config.model.init::<B>(&device).load_record(record);

    eprintln!("samples {:?}", samples.shape());
    println!("snr, mod, pred");

    let chunk_size = 1000;

    for start in (0..samples.clone().dims()[0]).step_by(chunk_size) {
        let end = (start + chunk_size).min(samples.dims()[0]);

        let samples = samples.clone().slice(s![start..end, .., ..]);

        let output = model.forward(samples);
        let predicted = output.argmax(1).flatten::<1>(0, 1);

        for (i, p) in predicted.into_data().iter::<B::FloatElem>().enumerate() {
            println!("{}, {}, {}", snrs[start + i], mods[start + i], p);
        }
    }
}

fn main() -> Result<()> {
    type B = WebGpu<f32, i32>;
    let device = burn::backend::wgpu::WgpuDevice::default();

    let samples = {
        let samples: Array3<f32> = ndarray_npy::read_npy("preprocessed_npy/samples.npy").unwrap();
        let shape = samples.shape();
        let samples = TensorData::new(samples.as_slice().unwrap().to_vec(), shape);
        Tensor::from_data(samples, &device)
    };

    let snrs: Array1<f32> = ndarray_npy::read_npy("preprocessed_npy/snrs.npy").unwrap();
    let mods: Array1<u8> = ndarray_npy::read_npy("preprocessed_npy/mods.npy").unwrap();

    infer::<B>("model", device, samples, snrs, mods);

    Ok(())
}
