#![recursion_limit = "512"]
use futuresdr_burn::TrainingConfig;
use futuresdr_burn::dataset::RadioDataset;
use futuresdr_burn::dataset::RadioDatasetBatcher;
use futuresdr_burn::model::McldnnConfig;
// use futuresdr_burn::simple_cnn::SimpleCNNConfig;
// use futuresdr_burn::simple_model::SimpleConfig;

use burn::backend::Autodiff;
use burn::backend::WebGpu;
use burn::backend::wgpu::WgpuDevice;
use burn::data::dataloader::DataLoaderBuilder;
use burn::module::Module;
use burn::optim::AdamConfig;
use burn::prelude::*;
use burn::record::CompactRecorder;
use burn::tensor::backend::AutodiffBackend;
use burn::train::LearnerBuilder;
use burn::train::LearningStrategy;
use burn::train::metric::AccuracyMetric;
use burn::train::metric::LossMetric;

fn create_artifact_dir(artifact_dir: &str) {
    std::fs::remove_dir_all(artifact_dir).ok();
    std::fs::create_dir_all(artifact_dir).ok();
}

pub fn train<B: AutodiffBackend>(artifact_dir: &str, config: TrainingConfig, device: B::Device) {
    create_artifact_dir(artifact_dir);
    config
        .save(format!("{artifact_dir}/config.json"))
        .expect("Config should be saved successfully");

    B::seed(&device, config.seed);

    let batcher = RadioDatasetBatcher::default();

    let dataloader_train = DataLoaderBuilder::new(batcher.clone())
        .batch_size(config.batch_size)
        .shuffle(config.seed)
        .num_workers(config.num_workers)
        .build(RadioDataset::train());

    let dataloader_test = DataLoaderBuilder::new(batcher)
        .batch_size(config.batch_size)
        .shuffle(config.seed)
        .num_workers(config.num_workers)
        .build(RadioDataset::test());

    let model = config.model.init::<B>(&device);
    println!("model {model}");

    let learner = LearnerBuilder::new(artifact_dir)
        .metric_train_numeric(AccuracyMetric::new())
        .metric_valid_numeric(AccuracyMetric::new())
        .metric_train_numeric(LossMetric::new())
        .metric_valid_numeric(LossMetric::new())
        .with_file_checkpointer(CompactRecorder::new())
        .learning_strategy(LearningStrategy::SingleDevice(device.clone()))
        .num_epochs(config.num_epochs)
        .grads_accumulation(4)
        .summary()
        .build(model, config.optimizer.init(), config.learning_rate);

    let result = learner.fit(dataloader_train, dataloader_test);

    result
        .model
        .save_file(format!("{artifact_dir}/model"), &CompactRecorder::new())
        .expect("Trained model should be saved successfully");
}

fn main() -> anyhow::Result<()> {
    type MyAutodiffBackend = Autodiff<WebGpu<f32>>;
    let device = WgpuDevice::default();

    train::<MyAutodiffBackend>(
        "model",
        TrainingConfig::new(McldnnConfig::new(), AdamConfig::new())
            .with_num_workers(1)
            .with_num_epochs(103)
            .with_seed(123)
            .with_learning_rate(0.001)
            .with_batch_size(100),
        device,
    );

    Ok(())
}
