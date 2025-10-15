use burn::prelude::*;
use futuresdr::prelude::*;
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
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        while let Some(b) = self.input.get_full_buffer() {
            self.input.put_empty_buffer(b);
            if self.start.is_none() {
                self.start = Some(Instant::now());
            }
        }

        if self.input.finished() {
            println!("took {:?}", self.start.unwrap().elapsed());
            io.finished = true;
        }

        Ok(())
    }
}
