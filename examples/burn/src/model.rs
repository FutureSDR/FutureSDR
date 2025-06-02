use burn::module::Module;
use burn::nn::Dropout;
use burn::nn::DropoutConfig;
use burn::nn::Linear;
use burn::nn::LinearConfig;
use burn::nn::Lstm;
use burn::nn::LstmConfig;
use burn::nn::PaddingConfig1d;
use burn::nn::PaddingConfig2d;
use burn::nn::conv::Conv1d;
use burn::nn::conv::Conv1dConfig;
use burn::nn::conv::Conv2d;
use burn::nn::conv::Conv2dConfig;
use burn::nn::loss::CrossEntropyLossConfig;
use burn::prelude::*;
use burn::tensor::activation::relu;
use burn::train::ClassificationOutput;

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
    dropout: Dropout,
}

#[derive(Config, Debug)]
pub struct McldnnConfig {
    #[config(default = "11")]
    num_classes: usize,
}

impl McldnnConfig {
    /// Returns the initialized model.
    pub fn init<B: Backend>(&self, device: &B::Device) -> Mcldnn<B> {
        // ──────── Branch 1 Conv2D ────────
        // Using [3, 9] for conv1_1 since we need odd kernel size, will pad explicitly in forward
        let conv1_1 = Conv2dConfig::new([1, 50], [3, 9])
            .with_padding(PaddingConfig2d::Valid)
            .init(device);

        // ──────── Branch 2 Conv1D ────────
        // Using kernel size 8 to match Keras exactly
        let conv1d_cfg = Conv1dConfig::new(8, 1, 50).with_padding(PaddingConfig1d::Valid);
        let conv1_2 = conv1d_cfg.init(device);
        let conv1_3 = conv1d_cfg.init(device);

        // ──────── Small Conv2D on merged Branch 2 ────────
        let conv2 = Conv2dConfig::new([1, 50], [1, 8])
            .with_padding(PaddingConfig2d::Valid)
            .init(device);

        // ──────── Big Conv2D after channel–concat ────────
        let conv4 = Conv2dConfig::new([1, 100], [2, 5])
            .with_padding(PaddingConfig2d::Valid)
            .init(device);

        let lstm1 = LstmConfig::new(100, 128, true).init(device);
        let lstm2 = LstmConfig::new(128, 128, true).init(device);
        let fc1 = LinearConfig::new(128, 128).init(device);
        let fc2 = LinearConfig::new(128, self.num_classes).init(device);
        let dropout = DropoutConfig::new(0.5).init();

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
            dropout,
        }
    }
}

/// Applies the “SELU” activation to `x`:
///   SELU(x) = λ * x,                     if x > 0
///           = λ * (α * exp(x) − α),      if x ≤ 0
///
/// where α ≈ 1.67326324 and λ ≈ 1.05070098.
pub fn selu<B: Backend>(x: Tensor<B, 2>) -> Tensor<B, 2> {
    // 1) Create scalar tensors for α and λ on the same device as `x`.
    let alpha = Tensor::from_data([1.6732632f32], &x.device());
    let lambda = Tensor::from_data([1.050701f32], &x.device());

    // 2) Build a “zero” tensor to compare x > 0.
    let zero = Tensor::from_data([0.0f32], &x.device());
    let mask_pos = x.clone().greater(zero.clone());
    //    mask_pos: a boolean‐mask Tensor<B, 2> where true = x>0

    // 3) Positive branch: λ * x
    let pos = x.clone() * lambda.clone();

    // 4) Negative branch: λ * (α * exp(x) − α)
    let neg = {
        let exp_x = x.clone().exp(); // eˣ
        let a_exp_x = alpha.clone() * exp_x; // α·eˣ
        let inner = a_exp_x - alpha.clone(); // α·eˣ − α
        lambda.clone() * inner // λ * (α·eˣ − α)
    };

    // 5) Combine the two branches using mask_pos:
    pos.mask_where(mask_pos, neg)
}

impl<B: Backend> Mcldnn<B> {
    pub fn forward(&self, inputs: (Tensor<B, 4>, Tensor<B, 3>, Tensor<B, 3>)) -> Tensor<B, 2> {
        let (input1, input2, input3) = inputs;

        // ──────── Branch 1: Conv2D(I/Q) ────────
        let mut x1 = self.conv1_1.forward(input1); // [batch,50,2,128]
        x1 = relu(x1);

        // ──────── Branch 2a: Conv1D on “real” ────────
        let mut x2 = self.conv1_2.forward(input2); // [batch,50,128]
        x2 = relu(x2);
        let x2 = x2.unsqueeze_dim(2); // → [batch,50,1,128]

        // ──────── Branch 2b: Conv1D on “imag” ────────
        let mut x3 = self.conv1_3.forward(input3); // [batch,50,128]
        x3 = relu(x3);
        let x3 = x3.unsqueeze_dim(2); // → [batch,50,1,128]

        // ──────── Merge Branch 2 vertically ────────
        // Stack on height axis (dim=2)
        let mut x23 = Tensor::cat(vec![x2, x3], 2); // [batch,50,2,128]
        x23 = self.conv2.forward(x23); // [batch,50,2,128]
        x23 = relu(x23);

        // ──────── Fuse Branch 1 & Branch 2 on channel axis ────────
        let x = Tensor::cat(vec![x1, x23], 1); // [batch,100,2,128]

        // ──────── Big Conv2D ────────
        let mut x = self.conv4.forward(x); // [batch,100,1,124]
        x = relu(x);

        // ──────── Reshape → LSTM input ────────
        let x = x.squeeze_dims(&[2]); // [batch,100,124]
        let x = x.permute([0, 2, 1]); // [batch,124,100]

        // ──────── LSTM #1 ────────
        let (x, _) = self.lstm1.forward(x.clone(), None); // [batch,124,128]
        // ──────── LSTM #2 ────────
        let (_, h2) = self.lstm2.forward(x, None); // [batch,124,128], h2: [1, batch, 128]

        // Grab layer-0’s hidden state (first dimension of h2)
        let h2 = h2.hidden;

        // ──────── Dense→SELU→Dropout→Dense head ────────
        let mut x = self.fc1.forward(h2.clone()); // [batch,128]
        x = selu(x);
        x = self.dropout.forward(x); // Dropout only in “train” mode
        self.fc2.forward(x) // [batch, num_classes]
    }

    pub fn forward_classification(
        &self,
        iq_samples: Tensor<B, 3>,
        modulations: Tensor<B, 1, Int>,
    ) -> ClassificationOutput<B> {
        let output = self.forward(iq_samples);
        let loss = CrossEntropyLossConfig::new()
            .init(&output.device())
            .forward(output.clone(), modulations.clone());

        ClassificationOutput::new(loss, output, modulations)
    }
}
