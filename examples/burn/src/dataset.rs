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
    iq_samples: Array2<f32>,
    modulation: u8,
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
    pub real: Tensor<B, 3>,
    pub imag: Tensor<B, 3>,
    pub iq_samples: Tensor<B, 4>,
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

        let real = items
            .iter()
            .map(|item| {
                let data = item.iq_samples.index_axis(Axis(0), 0).to_vec();
                let shape = vec![1, 1, item.iq_samples.shape()[1]];
                let d = TensorData::new(data, shape);
                Tensor::<B, 3>::from_data(d, device)
            })
            .collect();

        let imag = items
            .iter()
            .map(|item| {
                let data = item.iq_samples.index_axis(Axis(0), 1).to_vec();
                let shape = vec![1, 1, item.iq_samples.shape()[1]];
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

        let real = Tensor::cat(real, 0);
        let imag = Tensor::cat(imag, 0);
        let iq_samples = Tensor::cat(iq_samples, 0);
        let iq_samples = iq_samples.unsqueeze_dim(1);
        let modulation = Tensor::cat(modulation, 0);

        RadioDatasetBatch {
            real,
            imag,
            iq_samples,
            modulation,
        }
    }
}
