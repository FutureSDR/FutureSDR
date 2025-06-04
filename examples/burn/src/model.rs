use burn::module::Module;
use burn::nn::Dropout;
use burn::nn::DropoutConfig;
use burn::nn::Linear;
use burn::nn::LinearConfig;
use burn::nn::Lstm;
use burn::nn::LstmConfig;
use burn::nn::PaddingConfig1d;
use burn::nn::PaddingConfig2d;
use burn::nn::Relu;
use burn::nn::conv::Conv1d;
use burn::nn::conv::Conv1dConfig;
use burn::nn::conv::Conv2d;
use burn::nn::conv::Conv2dConfig;
use burn::nn::loss::CrossEntropyLossConfig;
use burn::prelude::*;
use burn::tensor::backend::AutodiffBackend;
use burn::train::ClassificationOutput;
use burn::train::TrainOutput;
use burn::train::TrainStep;
use burn::train::ValidStep;

use crate::dataset::RadioDatasetBatch;

/// Applies the “SELU” activation to `x`:
///   SELU(x) = λ * x,                     if x > 0
///           = λ * (α * exp(x) − α),      if x ≤ 0
///
/// where α ≈ 1.67326324 and λ ≈ 1.05070098.
pub fn selu<B: Backend>(x: Tensor<B, 2>) -> Tensor<B, 2> {
    let alpha = 1.6732632f32;
    let lambda = 1.050701f32;

    // 2) Build a “zero” tensor to compare x > 0.
    let zero = x.zeros_like();
    let mask_pos = x.clone().greater(zero).bool_not();

    // 3) Positive branch: λ * x
    let pos = x.clone() * lambda;

    // 4) Negative branch: λ * (α * exp(x) − α)
    let neg = {
        let exp_x = x.exp(); // eˣ
        let a_exp_x = exp_x * alpha; // α·eˣ
        let inner = a_exp_x - alpha; // α·eˣ − α
        inner * lambda // λ * (α·eˣ − α)
    };

    // 5) Combine the two branches using mask_pos:
    pos.mask_where(mask_pos, neg)
}

/// Mcldnn: replicates the Keras MCLDNN topology
///
///  - Branch 1: Conv2D on “[batch, 1, 2, 128]”  
///  - Branch 2: two Conv1D’s followed by a small Conv2D  
///  - Fuse → big Conv2D → reshape → two LSTMs → SELU+Dense head
#[derive(Module, Debug)]
pub struct Mcldnn<B: Backend> {
    // Branch 1 (I/Q 2×128 → Conv2D)
    conv1_1: Conv2d<B>,
    // Branch 2a/2b (each 1D on real/imag)
    conv1_2: Conv1d<B>,
    conv1_3: Conv1d<B>,
    // After merging branch 2 vertically, small Conv2D
    conv2: Conv2d<B>,
    // After channel-concat with branch 1, big Conv2D
    conv4: Conv2d<B>,
    // Two LSTM layers
    lstm1: Lstm<B>,
    lstm2: Lstm<B>,
    // Dense head
    fc1: Linear<B>,
    fc2: Linear<B>,
    fc3: Linear<B>,
    dropout: Dropout,
    relu: Relu,
}

#[derive(Config, Debug)]
pub struct McldnnConfig {
    #[config(default = "11")]
    num_classes: usize,
}

impl McldnnConfig {
    /// Returns the initialized model.
    pub fn init<B: AutodiffBackend>(&self, device: &B::Device) -> Mcldnn<B> {
        // ──────── Branch 1 Conv2D ────────
        // Use odd kernel [3,9], then Same padding → preserves [2,128]
        let conv1_1 = Conv2dConfig::new([1, 50], [2, 8])
            .with_padding(PaddingConfig2d::Valid)
            .init(device);
        // Input1: [batch, 1, 2, 128] → Output: [batch, 50, 2, 128]

        // ──────── Branch 2 Conv1D ────────
        // Use odd kernel 9 with Same padding → preserves length 128
        let conv1_2 = Conv1dConfig::new(1, 50, 8)
            .with_padding(PaddingConfig1d::Valid)
            .init(device);
        let conv1_3 = Conv1dConfig::new(1, 50, 8)
            .with_padding(PaddingConfig1d::Valid)
            .init(device);

        // ──────── Small Conv2D on merged Branch 2 ────────
        // After unsqueeze & vertical cat: [batch, 50, 2, 128]
        // Use [1,9], Same padding → preserves [2,128]
        let conv2 = Conv2dConfig::new([50, 50], [1, 8])
            .with_padding(PaddingConfig2d::Valid)
            .init(device);
        // [batch, 50, 2, 128] → stays [batch, 50, 2, 128]

        // ──────── Big Conv2D after channel–concat ────────
        // Now x1 & x23 both [batch, 50, 2, 128] ⇒ concatenated → [batch,100,2,128]
        // Use [2,5], Valid padding → height 2→1, width 128→124
        let conv4 = Conv2dConfig::new([100, 100], [2, 5])
            .with_padding(PaddingConfig2d::Valid)
            .init(device);
        // [batch,100,2,128] → [batch,100,1,124]

        let lstm1 = LstmConfig::new(100, 128, true).init(device);
        let lstm2 = LstmConfig::new(128, 128, true).init(device);
        let fc1 = LinearConfig::new(128, 128).init(device);
        let fc2 = LinearConfig::new(128, 128).init(device);
        let fc3 = LinearConfig::new(128, self.num_classes).init(device);
        let dropout = DropoutConfig::new(0.5).init();
        let relu = Relu::new();

        Mcldnn {
            conv1_1,
            conv1_2,
            conv1_3,
            conv2,
            conv4,
            lstm1,
            lstm2,
            fc1,
            fc2,
            fc3,
            dropout,
            relu,
        }
    }
}

impl<B: Backend> Mcldnn<B> {
    pub fn forward(&self, inputs: (Tensor<B, 4>, Tensor<B, 3>, Tensor<B, 3>)) -> Tensor<B, 2> {
        let (input1, input2, input3) = inputs;

        // ──────── Branch 1: Conv2D(I/Q) ────────
        let mut x1 = self.conv1_1.forward(input1.pad((7, 0, 0, 1), 0.0)); // [batch,50,2,128]
        x1 = self.relu.forward(x1);

        // ──────── Branch 2a: Conv1D on “real” ────────
        let mut x2 = self.conv1_2.forward(input2.pad((7, 0, 0, 0), 0.0)); // [batch,50,128]
        x2 = self.relu.forward(x2);
        let x2 = x2.unsqueeze_dim(2); // → [batch,50,1,128]
        // log::info!("x2 {:?}", x2.shape());

        // ──────── Branch 2b: Conv1D on “imag” ────────
        let mut x3 = self.conv1_3.forward(input3.pad((7, 0, 0, 0), 0.0)); // [batch,50,128]
        x3 = self.relu.forward(x3);
        let x3 = x3.unsqueeze_dim(2); // → [batch,50,1,128]
        // log::info!("x3 {:?}", x3.shape());

        // Stack x2 and x3 vertically
        let mut x23 = Tensor::cat(vec![x2, x3], 2); // [batch,128,2,50]
        // log::info!("x23 after cat {:?}", x23.shape());

        // ──────── Merge Branch 2 vertically ────────
        // Stack on height axis (dim=2)
        x23 = self.conv2.forward(x23.pad((7, 0, 0, 0), 0.0)); // [batch,50,2,128]
        // log::info!("x23 after conv {:?}", x23.shape());
        x23 = self.relu.forward(x23);

        // ──────── Fuse Branch 1 & Branch 2 on channel axis ────────
        let x = Tensor::cat(vec![x1, x23], 1); // [batch,100,2,128]
        // log::info!("x after cat {:?}", x.shape());

        // ──────── Big Conv2D ────────
        let mut x = self.conv4.forward(x); // [batch,100,1,124]
        x = self.relu.forward(x);
        // log::info!("x after conv4 {:?}", x.shape());

        // ──────── Reshape → LSTM input ────────
        let x: Tensor<B, 3> = x.squeeze_dims(&[2]); // [batch,100,124]
        // log::info!("x after squeeze {:?}", x.shape());
        let x = x.permute([0, 2, 1]); // [batch,124,100]
        // log::info!("x after permute {:?}", x.shape());

        // First LSTM - return full sequence
        let (x, _) = self.lstm1.forward(x, None); // [batch,124,128]

        // Second LSTM - need only final output
        let (_, h2) = self.lstm2.forward(x, None); // h2: [1,batch,128]
        let h2 = h2.hidden; // Get hidden state
        // log::info!("h2 {:?}", h2.shape());

        // ──────── Dense→SELU→Dropout→Dense→SELU→Dropout→Dense+Softmax head ────────
        let mut x = self.fc1.forward(h2); // [batch,128]
        x = selu(x);
        x = self.dropout.forward(x);

        let mut x = self.fc2.forward(x); // [batch,128]
        x = selu(x);
        x = self.dropout.forward(x);

        self.fc3.forward(x) // [batch,num_classes]
    }

    pub fn forward_classification(
        &self,
        real: Tensor<B, 3>,
        imag: Tensor<B, 3>,
        iq_samples: Tensor<B, 4>,
        modulations: Tensor<B, 1, Int>,
    ) -> ClassificationOutput<B> {
        let output = self.forward((iq_samples, real, imag));
        let loss = CrossEntropyLossConfig::new()
            .init(&output.device())
            .forward(output.clone(), modulations.clone());
        ClassificationOutput::new(loss, output, modulations)
    }
}

// struct PrintGrads<'a, B: AutodiffBackend> {
//     grads: &'a B::Gradients,
// }
//
// impl<'a, B: AutodiffBackend> burn::module::ModuleVisitor<B> for PrintGrads<'a, B> {
//     fn visit_float<const D: usize>(
//         &mut self,
//         id: burn::module::ParamId,
//         tensor: &Tensor<B, D>,
//     ) {
//         if let Some(grad_tensor) = tensor.grad(self.grads) {
//             log::info!("Grad for parameter {id:?}: {grad_tensor}");
//         } else {
//             log::warn!("No gradient found for parameter {id:?}");
//         }
//     }
// }

impl<B: AutodiffBackend> TrainStep<RadioDatasetBatch<B>, ClassificationOutput<B>> for Mcldnn<B> {
    fn step(&self, batch: RadioDatasetBatch<B>) -> TrainOutput<ClassificationOutput<B>> {
        let item =
            self.forward_classification(batch.real, batch.imag, batch.iq_samples, batch.modulation);

        let grads = item.loss.backward();
        // self.visit(&mut PrintGrads { grads: &grads });
        TrainOutput::new(self, grads, item)
    }
}

impl<B: Backend> ValidStep<RadioDatasetBatch<B>, ClassificationOutput<B>> for Mcldnn<B> {
    fn step(&self, batch: RadioDatasetBatch<B>) -> ClassificationOutput<B> {
        self.forward_classification(batch.real, batch.imag, batch.iq_samples, batch.modulation)
    }
}
