mod model;
use model::McldnnBurn;

use anyhow::Result;
use burn::module::Module;
use burn::tensor::Tensor;
use burn::tensor::activation::{relu, selu};
use burn::nn::loss::CrossEntropyLoss;
use burn::record::Recorder;
use burn_ndarray::{NdArrayBackend, NdArrayDevice};
use burn::data::dataloader::DataLoader;
use ndarray::Array4;
use npy::NpyData;
use std::fs;

/// An iterator that yields (X: Tensor<[batch,2,128,1]>, Y: one‐hot Tensor<[batch,11]>).
// struct AmrDataset {
//     files_x: Vec<String>,
//     files_y: Vec<String>,
//     idx: usize,
//     num_classes: usize,
// }

// impl AmrDataset {
//     fn new(dir: &str, num_classes: usize) -> Self {
//         let mut files_x = Vec::new();
//         let mut files_y = Vec::new();
//         for entry in fs::read_dir(dir).unwrap() {
//             let path = entry.unwrap().path();
//             let s = path.to_string_lossy().to_string();
//             if s.contains("/X_") && s.ends_with(".npy") {
//                 files_x.push(s.clone());
//             }
//             if s.contains("/Y_") && s.ends_with(".npy") {
//                 files_y.push(s.clone());
//             }
//         }
//         files_x.sort();
//         files_y.sort();
//         AmrDataset { files_x, files_y, idx: 0, num_classes }
//     }
// }

// impl Iterator for AmrDataset {
//     type Item = (Tensor<NdArrayBackend<f32>, 4>, Tensor<NdArrayBackend<f32>, 2>);
//
//     fn next(&mut self) -> Option<Self::Item> {
//         if self.idx >= self.files_x.len() {
//             return None;
//         }
//         let x_path = &self.files_x[self.idx];
//         let y_path = &self.files_y[self.idx];
//
//         // Load X: Array4<f32>, shape [batch, 2, 128, 1]
//         let x_arr: Array4<f32> = NpyData::from_file(x_path)
//             .unwrap()
//             .into_ndarray()
//             .unwrap();
//         let x_ndarray = burn_ndarray::NdArray::from_data(x_arr.into_dyn())
//             .into_dimensionality()
//             .unwrap();
//         let x_tensor = Tensor::from_data(x_ndarray.to_device(&NdArrayDevice::default()));
//
//         // Load Y: Vec<u8> of length batch; convert to one‐hot [batch, num_classes]
//         let y_vec: Vec<u8> = NpyData::from_file(y_path)
//             .unwrap()
//             .as_slice::<u8>()
//             .unwrap()
//             .to_vec();
//         let batch_size = y_vec.len();
//         let mut y_onehot = burn_ndarray::NdArray::<f32, 2>::zeros([batch_size, self.num_classes], &NdArrayDevice::default());
//         for (i, &lbl) in y_vec.iter().enumerate() {
//             y_onehot[[i, lbl as usize]] = 1.0;
//         }
//         let y_tensor = Tensor::from_data(y_onehot.to_device(&NdArrayDevice::default()));
//
//         self.idx += 1;
//         Some((x_tensor, y_tensor))
//     }
// }

fn main() -> Result<()> {
    // 1) Choose the backend (CPU via ndarray)
    // type B = NdArrayBackend<f32>;
    // let device = NdArrayDevice::default();
    //
    // // 2) Instantiate model & optimizer
    // let num_classes = 11;
    // let mut model = McldnnBurn::<B>::new(&device, num_classes);
    //
    // // Example: Use Adam (0.18’s API might differ slightly)
    // let mut optimizer = burn::optim::Adam::<B, _>::new(
    //     burn::optim::AdamConfig::new().with_lr(1e-3),
    //     model.parameters(),
    // );
    //
    // // 3) Build DataLoader
    // let train_data = AmrDataset::new("data/preprocessed_npy", num_classes);
    // let mut data_loader = DataLoader::new(train_data, /* batch_size= */ 32);
    //
    // // 4) Training loop (one epoch shown here)
    // while let Some((x_batch, y_batch)) = data_loader.next() {
    //     // We need to split x_batch into (input1,input2,input3).
    //     // Suppose your npy files already store (2, 128, 1) as “I/Q” interleaved,
    //     // or else you can just clone:
    //     let x1 = x_batch.clone();      // shape [batch, 2, 128, 1]
    //     let (x2, x3) = {
    //         // Example: split real/imag → two copies of shape [batch,1,128]
    //         let x2 = x_batch.slice([.., 0..1, .., ..]).to_owned(); // real
    //         let x3 = x_batch.slice([.., 1..2, .., ..]).to_owned(); // imag
    //         (x2, x3)
    //     };
    //
    //     // Forward pass
    //     let logits = model.forward((x1, x2, x3)); // [batch, num_classes]
    //
    //     // Compute loss
    //     let loss = CrossEntropyLoss::new().forward(logits.clone(), y_batch.clone());
    //
    //     // Backprop & step
    //     optimizer.backward_and_step(&loss);
    //
    //     // Print loss for this batch
    //     let loss_val: f32 = loss.to_data().scalar();
    //     println!("Batch loss = {:.4}", loss_val);
    // }
    //
    // // 5) Save parameters (no ONNX needed)
    // save("mcldnn_burn_params.bin", model.params())?;
    Ok(())
}
