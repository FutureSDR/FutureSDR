use crate::runtime::{Block, StreamIoBuilder, BlockMetaBuilder, MessageIoBuilder, AsyncKernel, WorkIo, StreamIo, MessageIo, BlockMeta};
use std::{mem, marker::PhantomData};
use async_trait::async_trait;
use anyhow::Result;

pub struct Decimation<T> where
    T: Sized + Sync + Send + std::fmt::Display + Clone + Copy + 'static {
    sample_rate: f32,
    decimation: usize,
    remaining_samples: usize,
    _x: PhantomData<T>
}

impl<T> Decimation<T> where
    T: Sized + Sync + Send + std::fmt::Display + Clone + Copy + 'static {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(sample_rate: f32, decimation: usize) -> Block {
        Block::new_async(
            BlockMetaBuilder::new("Decimation").build(),
            StreamIoBuilder::new()
                .add_input("in", mem::size_of::<T>())
                .add_output("out", mem::size_of::<T>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                sample_rate,
                decimation,
                remaining_samples: 0,
                _x: PhantomData
            },
        )
    }
}

#[async_trait]
impl<T> AsyncKernel for Decimation<T> where
    T: Sized + Sync + Send + std::fmt::Display + Clone + Copy + 'static {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let i = sio.input(0).slice::<T>();
        let o = sio.output(0).slice::<T>();

        let n = std::cmp::min(i.len(), o.len());

//        println!("Decimation: Running {} samples with {} remaining", n, self.remaining_samples);

        // Use iterator to remove every nth element
        let mut output_index = 0_usize;
        let mut remaining_samples = 0_usize;
        for index in self.remaining_samples..n {
            if (index - self.remaining_samples) % self.decimation == 0 {
                o[output_index] = i[index];
                output_index = output_index + 1;
                remaining_samples = 0;
            } else {
                remaining_samples = remaining_samples + 1;
            }
        }

        self.remaining_samples = remaining_samples;

//        println!("Finished Status: {}", sio.input(0).finished());

        if sio.input(0).finished() && ((i.len() - n) < self.decimation) {
//            println!("Decimation: Completed");
            io.finished = true;
        }

//        println!("Decimation: Producing {} samples.", output_index);


        sio.input(0).consume(n);
        sio.output(0).produce(output_index);

        if n < self.decimation - self.remaining_samples {
//            println!("Decimation: Exited early...");
            return Ok(());
        }

        Ok(())
    }
}

pub struct DecimationBuilder<T> where
    T: Sized + Sync + Send + std::fmt::Display + Clone + Copy + 'static {
    sample_rate: Option<f32>,
    decimation: Option<usize>,
    _x: PhantomData<T>
}

impl<T> DecimationBuilder<T> where
    T: Sized + Sync + Send + std::fmt::Display + Clone + Copy + 'static {
    pub fn new() -> DecimationBuilder<T> {
        DecimationBuilder {
            sample_rate: None,
            decimation: None,
            _x: PhantomData
        }
    }

    pub fn build(self) -> Block {

        if self.sample_rate.is_none() {
            println!("Sample rate is not defined for Decimation")
        }

        if self.decimation.is_none() {
            println!("Decimation is not defined for Decimation")
        }

        Decimation::<T>::new(self.sample_rate.unwrap(), self.decimation.unwrap())
    }

    pub fn sample_rate(mut self, rate: f32) -> DecimationBuilder<T> {
        self.sample_rate = Some(rate);
        self
    }

    pub fn decimation(mut self, decimation: usize) -> DecimationBuilder<T> {
        self.decimation = Some(decimation);
        self
    }
}

impl<T> Default for DecimationBuilder<T> where
    T: Sized + Sync + Send + std::fmt::Display + Clone + Copy + 'static {
    fn default() -> Self {
        Self::new()
    }
}
