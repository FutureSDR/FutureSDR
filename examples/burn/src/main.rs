mod model;
use model::McldnnConfig;

use burn::backend::Autodiff;
use burn::backend::ndarray::NdArray;
use burn::backend::ndarray::NdArrayDevice;
use burn::data::dataloader::DataLoaderBuilder;
use burn::data::dataloader::Dataset;
use burn::data::dataloader::batcher::Batcher;
use burn::module::Module;
use burn::nn::loss::CrossEntropyLossConfig;
use burn::optim::AdamConfig;
use burn::prelude::*;
use burn::record::CompactRecorder;
use burn::record::FullPrecisionSettings;
use burn::record::NamedMpkFileRecorder;
use burn::tensor::Tensor;
use burn::tensor::backend::AutodiffBackend;
use burn::train::LearnerBuilder;
use burn::train::metric::AccuracyMetric;
use burn::train::metric::LossMetric;
use ndarray::Array1;
use ndarray::Array2;
use ndarray::Array3;
use ndarray::Array4;
use ndarray::Axis;
use npy::NpyData;

#[derive(Clone, Debug)]
struct RadioDatasetItem {
    iq_samples: Array2<f32>,
    modulation: u8,
}

#[derive(Clone, Debug)]
struct RadioDataset {
    x: Array3<f32>,
    y: Array1<u8>,
}

impl RadioDataset {
    fn train() -> Self {
        let x_bytes = std::fs::read("preprocessed_npy/X_train").unwrap();
        let x_data: NpyData<f32> = NpyData::from_bytes(&x_bytes).unwrap();
        let x_arr: Array1<f32> = Array1::<f32>::from_iter(x_data.to_vec());
        let x: Array3<f32> = x_arr.into_shape_with_order((158400, 2, 128)).unwrap();
        let y_bytes = std::fs::read("preprocessed_npy/Y_train").unwrap();
        let y_data: NpyData<u8> = NpyData::from_bytes(&y_bytes).unwrap();
        let y: Array1<u8> = Array1::<u8>::from_iter(y_data.to_vec());

        Self { x, y }
    }
    fn test() -> Self {
        let x_bytes = std::fs::read("preprocessed_npy/X_test").unwrap();
        let x_data: NpyData<f32> = NpyData::from_bytes(&x_bytes).unwrap();
        let x_arr: Array1<f32> = Array1::<f32>::from_iter(x_data.to_vec());
        let x: Array3<f32> = x_arr.into_shape_with_order((22000, 2, 128)).unwrap();
        let y_bytes = std::fs::read("preprocessed_npy/Y_test").unwrap();
        let y_data: NpyData<u8> = NpyData::from_bytes(&y_bytes).unwrap();
        let y: Array1<u8> = Array1::<u8>::from_iter(y_data.to_vec());

        Self { x, y }
    }
}

impl Dataset<RadioDatasetItem> for RadioDataset {
    fn get(&self, index: usize) -> Option<RadioDatasetItem> {
        let s: Array2<f32> = self.x.index_axis(Axis(0), index).to_owned();

        Some(RadioDatasetItem {
            iq_samples: s,
            modulation: *self.y.get(index).unwrap(),
        })
    }

    fn len(&self) -> usize {
        self.x.shape()[0]
    }
}

#[derive(Clone, Default)]
pub struct RadioDatasetBatcher {}

#[derive(Clone, Debug)]
pub struct RadioDatasetBatch<B: Backend> {
    pub iq_samples: Tensor<B, 3>,
    pub modulation: Tensor<B, 1, Int>,
}

impl<B: Backend> Batcher<B, RadioDatasetItem, RadioDatasetBatch<B>> for RadioDatasetBatcher {
    fn batch(&self, items: Vec<RadioDatasetItem>, device: &B::Device) -> RadioDatasetBatch<B> {
        let iq_samples = items
            .iter()
            .map(|item| {
                let data = item.iq_samples.as_slice().unwrap().to_vec();
                let shape = vec![1, item.iq_samples.shape()[0], item.iq_samples.shape()[1]];
                let d = TensorData::new(data, shape);
                Tensor::<B, 3>::from_data(d, device)
            })
            .collect();

        let modulation = items
            .iter()
            .map(|item| {
                Tensor::<B, 1, Int>::from_data(
                    [(item.modulation as i64).elem::<B::IntElem>()],
                    device,
                )
            })
            .collect();

        let iq_samples = Tensor::cat(iq_samples, 0);
        let modulation = Tensor::cat(modulation, 0);

        RadioDatasetBatch {
            iq_samples,
            modulation,
        }
    }
}

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
        device.clone(),
    );

    Ok(())
}
