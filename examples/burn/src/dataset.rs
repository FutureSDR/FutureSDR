use burn::data::dataloader::Dataset;
use burn::data::dataloader::batcher::Batcher;
use burn::prelude::*;
use burn::tensor::Tensor;
use burn::tensor::TensorData;
use ndarray::Array1;
use ndarray::Array2;
use ndarray::Array3;
use ndarray::Axis;

#[derive(Clone, Debug)]
pub struct RadioDatasetItem {
    pub iq_samples: Array2<f32>,
    pub modulation: u8,
}

#[derive(Clone, Debug)]
pub struct RadioDataset {
    x: Array3<f32>,
    y: Array1<u8>,
}

impl RadioDataset {
    pub fn train() -> Self {
        let x: Array3<f32> = ndarray_npy::read_npy("preprocessed_npy/X_train.npy").unwrap();
        let y: Array1<u8> = ndarray_npy::read_npy("preprocessed_npy/Y_train.npy").unwrap();

        Self { x, y }
    }
    pub fn test() -> Self {
        let x: Array3<f32> = ndarray_npy::read_npy("preprocessed_npy/X_test.npy").unwrap();
        let y: Array1<u8> = ndarray_npy::read_npy("preprocessed_npy/Y_test.npy").unwrap();

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
                Tensor::<B, 1, Int>::from_data([item.modulation.elem::<B::IntElem>()], device)
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
