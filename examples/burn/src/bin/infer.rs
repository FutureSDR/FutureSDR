use anyhow::Result;
use burn::backend::WebGpu;
use burn::data::dataloader::Dataset;
use burn::data::dataloader::batcher::Batcher;
use futuresdr_burn::dataset::RadioDataset;
use futuresdr_burn::dataset::RadioDatasetBatch;
use futuresdr_burn::dataset::RadioDatasetBatcher;

fn main() -> Result<()> {
    type MyBackend = WebGpu<f32, i32>;
    let device = burn::backend::wgpu::WgpuDevice::default();

    let ds = RadioDataset::train();
    let item = ds.get(0).unwrap();
    println!("modulation {}", item.modulation);
    println!("samples {:?}", item.iq_samples);
    let item2 = ds.get(1).unwrap();
    println!("modulation {}", item2.modulation);
    println!("samples {:?}", item2.iq_samples);

    let batcher = RadioDatasetBatcher::default();
    let batch: RadioDatasetBatch<MyBackend> = batcher.batch(vec![item, item2], &device);

    println!("real {}", batch.real);
    println!("imag {}", batch.imag);
    println!("samples {}", batch.iq_samples);
    println!("modulation {}", batch.modulation);

    Ok(())
}
