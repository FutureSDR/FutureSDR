use burn::prelude::*;
use futuresdr::runtime::dev::prelude::*;
use std::time::Instant;

#[derive(Block)]
pub struct TimeIt<B: Backend> {
    start: Option<Instant>,
    #[input]
    input: burn_buffer::Reader<B>,
}

impl<B: Backend> TimeIt<B> {
    pub fn new() -> Self {
        Self {
            start: None,
            input: Default::default(),
        }
    }
}

impl<B: Backend> Default for TimeIt<B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B: Backend> Kernel for TimeIt<B> {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _b: &BlockMeta,
    ) -> Result<()> {
        let mut device = None;
        while let Some(b) = self.input.get_full_buffer() {
            if self.start.is_none() {
                self.start = Some(Instant::now());
            }

            let tensor = b.into_tensor();
            device = Some(tensor.device());
            drop(tensor);

            let device = device.as_ref().unwrap();
            B::sync(device)?;
            B::memory_cleanup(device);
        }

        if self.input.finished() {
            if let Some(device) = device.as_ref() {
                B::sync(device)?;
                B::memory_cleanup(device);
            }
            println!("took {:?}", self.start.unwrap().elapsed());
            io.finished = true;
        }

        Ok(())
    }
}
