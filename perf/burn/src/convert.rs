use burn::prelude::*;
use burn_buffer::Buffer;
use bytemuck::Pod;
use futuresdr::runtime::dev::prelude::*;

use crate::FFT_SIZE;

#[derive(Block)]
pub struct Convert<B: Backend>
where
    B::FloatElem: Pod,
{
    #[input]
    input: circular::Reader<Complex32>,
    #[output]
    output: burn_buffer::Writer<B, Float>,
    current: Option<(Buffer<B, Float>, usize)>,
    batch_size: usize,
}

impl<B: Backend> Convert<B>
where
    B::FloatElem: Pod,
{
    pub fn new(batch_size: usize) -> Self {
        Self {
            input: Default::default(),
            output: Default::default(),
            current: None,
            batch_size,
        }
    }
}

impl<B: Backend> Kernel for Convert<B>
where
    B::FloatElem: Pod,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _b: &BlockMeta,
    ) -> Result<()> {
        if self.current.is_none() {
            if let Some(mut b) = self.output.get_empty_buffer() {
                let items = self.batch_size * FFT_SIZE * 2;
                assert_eq!(b.num_host_elements(), items);
                b.set_valid(items);
                self.current = Some((b, 0));
            } else {
                if self.input.finished() {
                    io.finished = true;
                }
                return Ok(());
            }
        }

        let (buffer, offset) = self.current.as_mut().unwrap();
        let output = &mut buffer.slice()[*offset..];
        let input = self.input.slice();

        let input_len = input.len();

        let m = std::cmp::min(input.len(), output.len() / 2);
        for i in 0..m {
            output[2 * i] = input[i].re;
            output[2 * i + 1] = input[i].im;
        }

        *offset += 2 * m;
        self.input.consume(m);

        if m == output.len() / 2 {
            let (b, _) = self.current.take().unwrap();
            self.output.put_full_buffer(b)?;
            if self.output.has_more_buffers() {
                io.call_again = true;
            }
        }

        if self.input.finished() && m == input_len {
            io.finished = true;
        }

        Ok(())
    }
}
