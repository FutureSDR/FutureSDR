mod model;
use model::Mcldnn;

use burn::backend::ndarray::NdArray;
use burn::backend::ndarray::NdArrayDevice;
use burn::data::dataloader::DataLoader;
use burn::module::Module;
use burn::nn::loss::CrossEntropyLossConfig;
use burn::optim::AdamConfig;
use burn::record::FullPrecisionSettings;
use burn::record::NamedMpkFileRecorder;
use burn::tensor::Tensor;
use ndarray::Array;
use ndarray::Array4;
use npy::NpyData;
use std::fs;
use std::path::PathBuf;

use crate::model::McldnnConfig;

// /// (Same `AmrDataset` as in the previous snippet.)
// pub struct AmrDataset {
//     files_x: Vec<PathBuf>,
//     files_y: Vec<PathBuf>,
//     idx: usize,
//     num_classes: usize,
// }
//
// impl AmrDataset {
//     pub fn new(dir: &str, num_classes: usize) -> Self {
//         let mut files_x: Vec<PathBuf> = Vec::new();
//         let mut files_y: Vec<PathBuf> = Vec::new();
//
//         for entry in fs::read_dir(dir).unwrap() {
//             let path = entry.unwrap().path();
//             let s = path.to_string_lossy();
//
//             if s.contains("/X_") && s.ends_with(".npy") {
//                 files_x.push(path.clone());
//             }
//             if s.contains("/y_") && s.ends_with(".npy") {
//                 files_y.push(path.clone());
//             }
//         }
//
//         files_x.sort();
//         files_y.sort();
//
//         AmrDataset {
//             files_x,
//             files_y,
//             idx: 0,
//             num_classes,
//         }
//     }
// }
//
// impl Iterator for AmrDataset {
//     type Item = (Tensor<NdArray<f32>, 4>, Tensor<NdArray<f32>, 2>);
//
//     fn next(&mut self) -> Option<Self::Item> {
//         if self.idx >= self.files_x.len() {
//             return None;
//         }
//
//         // 1) Load X_batch
//         let x_path = &self.files_x[self.idx];
//         // let x_data: NpyData<f32> = NpyData::from_bytes(&std::fs::read(x_path).unwrap()).unwrap();
//         // let x_nd: ndarray::ArrayD<f32> = x_data.into_ndarray().unwrap();
//         let x_nd: ndarray::ArrayD<f32> = ndarray_npy::read_npy(x_path).unwrap();
//         let shape3 = x_nd.shape();
//         assert!(
//             shape3.len() == 3,
//             "Expected X to have shape [batch, 2, 128]"
//         );
//         let batch_size = shape3[0];
//         // Convert shape (batch, 2, 128) → (batch, 2, 128, 1)
//         let mut x_arr4 = Array4::zeros([batch_size, shape3[1], shape3[2], 1]);
//         for b in 0..batch_size {
//             for c in 0..shape3[1] {
//                 for w in 0..shape3[2] {
//                     x_arr4[[b, c, w, 0]] = x_nd[[b, c, w]];
//                 }
//             }
//         }
//         let x_ndarray: Array4<f32> = x_arr4.into();
//
//         let x_tensor: Tensor<Array4<f32>> =
//             Tensor::from_data(x_ndarray.to_device(&NdArrayDevice::default()));
//
//         // 2) Load Y_batch and one‐hot
//         let y_path = &self.files_y[self.idx];
//         let y_data: NpyData<u8> = NpyData::from_file(y_path).unwrap();
//         let y_slice: &[u8] = y_data.as_slice().unwrap();
//         assert!(y_slice.len() == batch_size, "Batch‐size disagreement");
//         let mut y_onehot_nd =
//             Array::<f32, 2>::zeros([batch_size, self.num_classes], &NdArrayDevice::default());
//         for (i, &lbl) in y_slice.iter().enumerate() {
//             y_onehot_nd[[i, lbl as usize]] = 1.0;
//         }
//         let y_tensor: Tensor<NdArray<f32>, 2> =
//             Tensor::from_data(y_onehot_nd.to_device(&NdArrayDevice::default()));
//
//         self.idx += 1;
//         Some((x_tensor, y_tensor))
//     }
// }

fn main() -> anyhow::Result<()> {
    // 1) Choose the backend (CPU/ndarray)
    type B = NdArray<f32>;
    let device = NdArrayDevice::default();

    // 2) Instantiate the model & optimizer
    let num_classes = 11;
    let model = McldnnConfig::new().with_num_classes(num_classes).init::<B>(&device);
    println!("{model}");

    // // Learning rate, Adam etc.
    // let mut optimizer = AdamConfig::new().init();
    //
    // // 3) Build DataLoader over our preprocessed_npy directory
    // let train_data = AmrDataset::new("preprocessed_npy", num_classes);
    // let mut data_loader = DataLoader::new(train_data, /* batch_size= */ 32);
    //
    // // 4) One epoch of training
    // while let Some((x_batch, y_batch)) = data_loader.next() {
    //     // The model’s `forward` signature expects (X_iq, X_real, X_imag).
    //     // We saved `x_batch` as shape [batch, 2, 128, 1], where `2`=I/Q.
    //     // For “real” and “imag” Conv1D branches, we need to split channels=2 → two tensors.
    //
    //     // Note: x_batch.dim() = TensorDim<4> = [batch, 2, 128, 1]
    //     // We index slice(.., 0..1, .., ..) for “real” (channel=0), slice(.., 1..2, .., ..) for “imag”.
    //
    //     // 1) Keep the full I/Q for Conv2d‐branch (Keras used the raw 2×128 as a single Conv2D input)
    //     let x1 = x_batch.clone(); // [batch, 2, 128, 1]
    //
    //     // 2) split out real vs. imag for Conv1D branches:
    //     let x2 = x_batch.slice([.., 0..1, .., ..]).to_owned(); // [batch, 1, 128, 1]
    //     let x3 = x_batch.slice([.., 1..2, .., ..]).to_owned(); // [batch, 1, 128, 1]
    //
    //     // BUT: Our Burn implementation’s Conv1d steps expected shape [batch, 1, 128],
    //     // not [batch, 1, 128, 1]. So we must squeeze the last dim:
    //     let x2 = x2.squeeze_dim(3); // [batch, 1, 128]
    //     let x3 = x3.squeeze_dim(3); // [batch, 1, 128]
    //
    //     // 3) Forward pass
    //     let logits: Tensor<NdArray<f32>, 2> = model.forward((x1, x2, x3)); // [batch, num_classes]
    //
    //     // 4) Compute cross‐entropy loss
    //     let loss = CrossEntropyLossConfig::new()
    //         .init(&device.clone())
    //         .forward(logits.clone(), y_batch.clone());
    //
    //     // 5) Backprop + optimizer step
    //     optimizer.backward_and_step(&loss);
    //
    //     // 6) Print batch‐loss
    //     let loss_val: f32 = loss.to_data().scalar();
    //     println!("Batch loss = {:.4}", loss_val);
    // }
    //
    // // 5) Save model in MessagePack format with full precision
    // let recorder = NamedMpkFileRecorder::<FullPrecisionSettings>::new();
    // model
    //     .save_file("model.mpk", &recorder)
    //     .expect("Should be able to save the model");

    Ok(())
}
