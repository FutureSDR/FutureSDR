#![recursion_limit = "512"]
pub mod dataset;
pub mod fft;
pub mod model;
pub mod simple_cnn;
pub mod simple_model;

use burn::optim::AdamConfig;
use burn::prelude::*;
use model::McldnnConfig;

pub const FFT_SIZE: usize = 2048;
pub const BATCH_SIZE: usize = 256;

#[derive(Config, Debug)]
pub struct TrainingConfig {
    pub model: McldnnConfig,
    // pub model: SimpleConfig,
    // pub model: SimpleCNNConfig,
    pub optimizer: AdamConfig,
    #[config(default = 10)]
    pub num_epochs: usize,
    #[config(default = 32)]
    pub batch_size: usize,
    #[config(default = 4)]
    pub num_workers: usize,
    #[config(default = 42)]
    pub seed: u64,
    #[config(default = 0.0001)]
    pub learning_rate: f64,
}
