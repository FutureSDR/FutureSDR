use futuresdr::runtime::dev::prelude::*;
use std::cmp::min;

/// Duplicate one input stream into `N` output streams.
///
/// # Stream Inputs
///
/// `input`: Input samples.
///
/// # Stream Outputs
///
/// `outputs[0]`, `outputs[1]`, ...: Copies of the input stream.
///
/// # Usage
/// ```
/// use futuresdr::blocks::StreamDuplicator;
///
/// let duplicator = StreamDuplicator::<f32, 2>::new();
/// ```
#[derive(Block)]
pub struct StreamDuplicator<
    T,
    const N: usize,
    I: CpuBufferReader<Item = T> = DefaultCpuReader<T>,
    O: CpuBufferWriter<Item = T> = DefaultCpuWriter<T>,
> {
    #[input]
    input: I,
    #[output]
    outputs: [O; N],
}

impl<T, const N: usize, I, O> StreamDuplicator<T, N, I, O>
where
    T: Copy + Send + Sync + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    /// Create Stream Duplicator.
    pub fn new() -> Self {
        Self {
            input: I::default(),
            outputs: std::array::from_fn(|_| O::default()),
        }
    }

    /// Create Stream Duplicator with a minimum output buffer size.
    pub fn with_min_output_buffer_size(min_items: usize) -> Self {
        let mut outputs = std::array::from_fn(|_| O::default());
        for output in outputs.iter_mut() {
            output.set_min_buffer_size_in_items(min_items);
        }
        Self {
            input: I::default(),
            outputs,
        }
    }
}

impl<T, const N: usize, I, O> Default for StreamDuplicator<T, N, I, O>
where
    T: Copy + Send + Sync + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    fn default() -> Self {
        Self::new()
    }
}

#[doc(hidden)]
impl<T, const N: usize, I, O> Kernel for StreamDuplicator<T, N, I, O>
where
    T: Copy + Send + Sync + 'static,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _b: &BlockMeta,
    ) -> Result<()> {
        let (input, tags) = self.input.slice_with_tags();
        let nitem_to_consume = input.len();
        let n_items_to_produce = self
            .outputs
            .iter_mut()
            .map(|x| x.slice().len())
            .min()
            .unwrap();
        let nitem_to_process = min(n_items_to_produce, nitem_to_consume);
        if nitem_to_process > 0 {
            for j in 0..N {
                let (out, mut out_tags) = self.outputs[j].slice_with_tags();
                out[..nitem_to_process].copy_from_slice(&input[..nitem_to_process]);
                tags.iter()
                    .filter(|t| t.index < nitem_to_process)
                    .for_each(|t| out_tags.add_tag(t.index, t.tag.clone()));
                self.outputs[j].produce(nitem_to_process);
            }
            self.input.consume(nitem_to_process);
        }
        if nitem_to_consume - nitem_to_process == 0 && self.input.finished() {
            io.finished = true;
        }
        Ok(())
    }
}
