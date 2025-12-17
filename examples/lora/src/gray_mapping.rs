use crate::utils::DemodulatedSymbol;
use crate::utils::DemodulatedSymbolHardDecoding;
use crate::utils::DemodulatedSymbolSoftDecoding;
use futuresdr::prelude::*;
use std::cmp::min;
use std::collections::HashMap;
use std::collections::VecDeque;

#[derive(Block)]
pub struct GrayMapping<
    T = DemodulatedSymbolSoftDecoding,
    I = DefaultCpuReader<T>,
    O = DefaultCpuWriter<T>,
> where
    T: DemodulatedSymbol,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    #[input]
    input: I,
    #[output]
    output: O,
}

impl<T, I, O> Default for GrayMapping<T, I, O>
where
    T: DemodulatedSymbol,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T, I, O> GrayMapping<T, I, O>
where
    T: DemodulatedSymbol,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    pub fn new() -> Self {
        Self {
            input: I::default(),
            output: O::default(),
        }
    }
}

trait GrayMap<T: DemodulatedSymbol>: Send {
    fn map(samples: &[T]) -> Vec<T>;
}

impl<I, O> GrayMap<DemodulatedSymbolHardDecoding>
    for GrayMapping<DemodulatedSymbolHardDecoding, I, O>
where
    I: CpuBufferReader<Item = DemodulatedSymbolHardDecoding>,
    O: CpuBufferWriter<Item = DemodulatedSymbolHardDecoding>,
{
    fn map(samples: &[DemodulatedSymbolHardDecoding]) -> Vec<DemodulatedSymbolHardDecoding> {
        samples
            .iter()
            .map(|x| *x ^ (*x >> 1))
            .collect::<Vec<DemodulatedSymbolHardDecoding>>()
    }
}

impl<I, O> GrayMap<DemodulatedSymbolSoftDecoding>
    for GrayMapping<DemodulatedSymbolSoftDecoding, I, O>
where
    I: CpuBufferReader<Item = DemodulatedSymbolSoftDecoding>,
    O: CpuBufferWriter<Item = DemodulatedSymbolSoftDecoding>,
{
    fn map(samples: &[DemodulatedSymbolSoftDecoding]) -> Vec<DemodulatedSymbolSoftDecoding> {
        // No gray mapping , it has as been done directly in fft_demod block => block "bypass"
        samples.to_vec()
    }
}

impl<T, I, O> Kernel for GrayMapping<T, I, O>
where
    T: DemodulatedSymbol,
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
    GrayMapping<T, I, O>: GrayMap<T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let (input, in_tags) = self.input.slice_with_tags();
        let (output, mut out_tags) = self.output.slice_with_tags();
        let input_len = input.len();
        let output_len = output.len();

        let mut nitems_to_process = min(input_len, output_len);
        if nitems_to_process == 0 {
            if self.input.finished() {
                io.finished = true;
            }
            return Ok(());
        }
        let mut tags: VecDeque<(usize, HashMap<String, Pmt>)> = in_tags
            .iter()
            .filter_map(|x| match x {
                ItemTag {
                    index,
                    tag: Tag::NamedAny(n, val),
                } => {
                    if n == "frame_info" {
                        match (**val).downcast_ref().unwrap() {
                            Pmt::MapStrPmt(map) => Some((*index, map.clone())),
                            _ => None,
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();

        if !tags.is_empty() {
            if tags[0].0 != 0 {
                nitems_to_process = tags[0].0; // only use symbol until the next frame begin (SF might change)
            } else {
                if tags.len() >= 2 {
                    nitems_to_process = tags[1].0; //  - tags[0].0; (== 0)
                }

                out_tags.add_tag(
                    0,
                    Tag::NamedAny(
                        "frame_info".to_string(),
                        Box::new(Pmt::MapStrPmt(tags.pop_front().unwrap().1)),
                    ),
                );
            }
        }

        let input = self.input.slice();
        let output = self.output.slice();
        output[0..nitems_to_process].copy_from_slice(&Self::map(&input[0..nitems_to_process]));

        self.input.consume(nitems_to_process);
        self.output.produce(nitems_to_process);

        if self.input.finished() && nitems_to_process == input_len {
            io.finished = true;
        }

        Ok(())
    }
}
