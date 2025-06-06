use burn::module::Module;
use burn::nn::Linear;
use burn::nn::LinearConfig;
use burn::nn::Relu;
use burn::nn::loss::CrossEntropyLossConfig;
use burn::prelude::*;
use burn::tensor::backend::AutodiffBackend;
use burn::train::ClassificationOutput;
use burn::train::TrainOutput;
use burn::train::TrainStep;
use burn::train::ValidStep;

use crate::dataset::RadioDatasetBatch;

#[derive(Module, Debug)]
pub struct Simple<B: Backend> {
    layer1: Linear<B>,
    relu: Relu,
    layer2: Linear<B>,
}

#[derive(Config, Debug)]
pub struct SimpleConfig {
    #[config(default = "11")]
    num_classes: usize,
    #[config(default = "256")]
    input_dim: usize,
    #[config(default = "512")]
    hidden_dim: usize,
}

impl SimpleConfig {
    pub fn init<B: Backend>(&self, device: &B::Device) -> Simple<B> {
        Simple {
            layer1: LinearConfig::new(self.input_dim, self.hidden_dim).init(device),
            relu: Relu::new(),
            layer2: LinearConfig::new(self.hidden_dim, self.num_classes).init(device),
        }
    }
}

impl<B: Backend> Simple<B> {
    pub fn forward(&self, input: Tensor<B, 2>) -> Tensor<B, 2> {
        let hidden = self.layer1.forward(input);
        let activated = self.relu.forward(hidden);
        self.layer2.forward(activated)
    }

    pub fn forward_classification(
        &self,
        iq_samples: Tensor<B, 3>,
        modulations: Tensor<B, 1, Int>,
    ) -> ClassificationOutput<B> {
        let batch_size = iq_samples.dims()[0];
        let x = iq_samples.reshape([batch_size, 2 * 128]);

        let output = self.forward(x);
        let loss = CrossEntropyLossConfig::new()
            .init(&output.device())
            .forward(output.clone(), modulations.clone());
        ClassificationOutput::new(loss, output, modulations)
    }
}

impl<B: AutodiffBackend> TrainStep<RadioDatasetBatch<B>, ClassificationOutput<B>> for Simple<B> {
    fn step(&self, batch: RadioDatasetBatch<B>) -> TrainOutput<ClassificationOutput<B>> {
        let item =
            self.forward_classification(batch.iq_samples, batch.modulation);
        let grads = item.loss.backward();
        TrainOutput::new(self, grads, item)
    }
}

impl<B: Backend> ValidStep<RadioDatasetBatch<B>, ClassificationOutput<B>> for Simple<B> {
    fn step(&self, batch: RadioDatasetBatch<B>) -> ClassificationOutput<B> {
        self.forward_classification(batch.iq_samples, batch.modulation)
    }
}
