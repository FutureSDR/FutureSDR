use burn::module::Module;
use burn::nn::Linear;
use burn::nn::LinearConfig;
use burn::nn::PaddingConfig1d;
use burn::nn::Relu;
use burn::nn::conv::Conv1d;
use burn::nn::conv::Conv1dConfig;
use burn::nn::loss::CrossEntropyLossConfig;
use burn::nn::pool::MaxPool1d;
use burn::nn::pool::MaxPool1dConfig;
use burn::prelude::*;
use burn::tensor::backend::AutodiffBackend;
use burn::train::ClassificationOutput;
use burn::train::TrainOutput;
use burn::train::TrainStep;
use burn::train::ValidStep;

use crate::dataset::RadioDatasetBatch;

#[derive(Module, Debug)]
pub struct SimpleCNN<B: Backend> {
    conv1: Conv1d<B>,
    relu1: Relu,
    pool1: MaxPool1d,
    conv2: Conv1d<B>,
    relu2: Relu,
    pool2: MaxPool1d,
    fc1: Linear<B>,
    relu3: Relu,
    fc2: Linear<B>,
}

#[derive(Config, Debug)]
pub struct SimpleCNNConfig {
    #[config(default = "11")]
    pub num_classes: usize,
    #[config(default = "2")]
    pub in_channels: usize,
    #[config(default = "128")]
    pub seq_len: usize,
    #[config(default = "128")]
    pub conv_channels1: usize,
    #[config(default = "256")]
    pub conv_channels2: usize,
    #[config(default = "256")]
    pub fc_hidden: usize,
}

impl SimpleCNNConfig {
    pub fn init<B: AutodiffBackend>(&self, device: &B::Device) -> SimpleCNN<B> {
        // conv1: in_channels=2, out_channels=64, kernel_size=3, padding=1 → preserves seq_len
        let conv1 = Conv1dConfig::new(self.in_channels, self.conv_channels1, 5)
            .with_padding(PaddingConfig1d::Same)
            .init(device);
        let pool1 = MaxPool1dConfig::new(2)
            .with_stride(2)
            .init(); // halves seq_len → 64
        // conv2: in_channels=64, out_channels=128, kernel_size=3, padding=1 → seq_len 64→64
        let conv2 = Conv1dConfig::new(self.conv_channels1, self.conv_channels2, 5)
            .with_padding(PaddingConfig1d::Same)
            .init(device);
        let pool2 = MaxPool1dConfig::new(2)
            .with_stride(2)
            .init(); // halves again → 32
        // fc layers: input_dim = conv_channels2 * (seq_len / 4) = 128 * 32 = 4096
        let fc1 = LinearConfig::new(self.conv_channels2 * (self.seq_len / 4), self.fc_hidden)
            .init(device);
        let fc2 = LinearConfig::new(self.fc_hidden, self.num_classes).init(device);
        SimpleCNN {
            conv1,
            relu1: Relu::new(),
            pool1,
            conv2,
            relu2: Relu::new(),
            pool2,
            fc1,
            relu3: Relu::new(),
            fc2,
        }
    }
}

impl<B: Backend> SimpleCNN<B> {
    /// Forward pass: input shape [batch, 2 (channels), 128 (seq_len)]
    pub fn forward(&self, input: Tensor<B, 3>) -> Tensor<B, 2> {
        // log::info!("FORWARD PASS");
        // log::info!("input {:?}", input.shape());
        // Conv1 + ReLU + Pool → shape [batch, 64, 64]
        let x = self.conv1.forward(input);
        // log::info!("after conv1 {:?}", x.shape());
        let x = self.relu1.forward(x);
        let x = self.pool1.forward(x);
        // log::info!("after pool1 {:?}", x.shape());
        // Conv2 + ReLU + Pool → shape [batch, 128, 32]
        let x = self.conv2.forward(x);
        // log::info!("after conv2 {:?}", x.shape());
        let x = self.relu2.forward(x);
        let x = self.pool2.forward(x);
        // Flatten: [batch, 128 * 32]
        // log::info!("after pool2 {:?}", x.shape());
        let batch_size = x.dims()[0];
        let s = x.dims()[2];
        let flattened = x.reshape([batch_size, 256 * s]);
        // log::info!("after flatten {:?}", flattened.shape());
        // FC 1 + ReLU
        let hidden = self.fc1.forward(flattened);
        let hidden = self.relu3.forward(hidden);
        // FC 2 → logits [batch, num_classes]
        self.fc2.forward(hidden)
    }

    pub fn forward_classification(
        &self,
        iq_samples: Tensor<B, 4>,
        modulations: Tensor<B, 1, Int>,
    ) -> ClassificationOutput<B> {
        let x: Tensor<B, 3> = iq_samples.squeeze(1); // -> [batch, 2, 128]
        let output = self.forward(x);
        let loss = CrossEntropyLossConfig::new()
            .init(&output.device())
            .forward(output.clone(), modulations.clone());
        ClassificationOutput::new(loss, output, modulations)
    }
}

impl<B: AutodiffBackend> TrainStep<RadioDatasetBatch<B>, ClassificationOutput<B>> for SimpleCNN<B> {
    fn step(&self, batch: RadioDatasetBatch<B>) -> TrainOutput<ClassificationOutput<B>> {
        let item = self.forward_classification(batch.iq_samples, batch.modulation);
        let grads = item.loss.backward();
        TrainOutput::new(self, grads, item)
    }
}

impl<B: Backend> ValidStep<RadioDatasetBatch<B>, ClassificationOutput<B>> for SimpleCNN<B> {
    fn step(&self, batch: RadioDatasetBatch<B>) -> ClassificationOutput<B> {
        self.forward_classification(batch.iq_samples, batch.modulation)
    }
}
