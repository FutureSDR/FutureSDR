pub mod dataset;
mod model;
use model::McldnnConfig;

use dataset::RadioDatasetBatcher;
use dataset::RadioDataset;

use burn::backend::Autodiff;
use burn::backend::ndarray::NdArray;
use burn::backend::ndarray::NdArrayDevice;
use burn::data::dataloader::DataLoaderBuilder;
use burn::module::Module;
use burn::optim::AdamConfig;
use burn::prelude::*;
use burn::record::CompactRecorder;
use burn::tensor::backend::AutodiffBackend;
use burn::train::LearnerBuilder;
use burn::train::metric::AccuracyMetric;
use burn::train::metric::LossMetric;

#[derive(Config)]
pub struct TrainingConfig {
    pub model: McldnnConfig,
    pub optimizer: AdamConfig,
    #[config(default = 10)]
    pub num_epochs: usize,
    #[config(default = 32)]
    pub batch_size: usize,
    #[config(default = 4)]
    pub num_workers: usize,
    #[config(default = 42)]
    pub seed: u64,
    #[config(default = 0.001)]
    pub learning_rate: f64,
}

fn create_artifact_dir(artifact_dir: &str) {
    // Remove existing artifacts before to get an accurate learner summary
    std::fs::remove_dir_all(artifact_dir).ok();
    std::fs::create_dir_all(artifact_dir).ok();
}

pub fn train<B: AutodiffBackend>(artifact_dir: &str, config: TrainingConfig, device: B::Device) {
    create_artifact_dir(artifact_dir);
    config
        .save(format!("{artifact_dir}/config.json"))
        .expect("Config should be saved successfully");

    B::seed(config.seed);

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

    let learner = LearnerBuilder::new(artifact_dir)
        .metric_train_numeric(AccuracyMetric::new())
        .metric_valid_numeric(AccuracyMetric::new())
        .metric_train_numeric(LossMetric::new())
        .metric_valid_numeric(LossMetric::new())
        .with_file_checkpointer(CompactRecorder::new())
        .devices(vec![device.clone()])
        .num_epochs(config.num_epochs)
        .summary()
        .build(
            config.model.init::<B>(&device),
            config.optimizer.init(),
            config.learning_rate,
        );

    let model_trained = learner.fit(dataloader_train, dataloader_test);

    model_trained
        .save_file(format!("{artifact_dir}/model"), &CompactRecorder::new())
        .expect("Trained model should be saved successfully");
}

fn main() -> anyhow::Result<()> {
    // 1) Choose the backend (CPU/ndarray)
    type B = NdArray<f32>;
    let device = NdArrayDevice::default();

    // 2) Instantiate the model & optimizer
    let num_classes = 11;
    let model = McldnnConfig::new()
        .with_num_classes(num_classes)
        .init::<B>(&device);
    println!("{model}");

    type MyBackend = NdArray<f32, i32>;
    type MyAutodiffBackend = Autodiff<MyBackend>;

    let device = NdArrayDevice::Cpu;
    let artifact_dir = "model";
    train::<MyAutodiffBackend>(
        artifact_dir,
        TrainingConfig::new(McldnnConfig::new(), AdamConfig::new()),
        device,
    );

    Ok(())
}
