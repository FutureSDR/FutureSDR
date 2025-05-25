use futuresdr::prelude::*;
use std::cmp::min;
use std::collections::HashMap;
use std::collections::VecDeque;

#[derive(Block)]
pub struct GrayMapping<I = DefaultCpuReader<u16>, O = DefaultCpuWriter<u16>>
where
    I: CpuBufferReader<Item = u16>,
    O: CpuBufferWriter<Item = u16>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    _m_soft_decoding: bool, // Hard/Soft decoding
}

impl<I, O> GrayMapping<I, O>
where
    I: CpuBufferReader<Item = u16>,
    O: CpuBufferWriter<Item = u16>,
{
    pub fn new(soft_decoding: bool) -> Self {
        // if soft_decoding {
        //     sio = sio.add_input::<[LLR; MAX_SF]>("in");
        //     sio = sio.add_output::<[LLR; MAX_SF]>("out");
        // } else {
        //     sio = sio.add_input::<u16>("in");
        //     sio = sio.add_output::<u16>("out");
        // }
        Self {
            input: I::default(),
            output: O::default(),
            _m_soft_decoding: soft_decoding,
        }
    }
}

impl<I, O> Kernel for GrayMapping<I, O>
where
    I: CpuBufferReader<Item = u16>,
    O: CpuBufferWriter<Item = u16>,
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

        // if self.m_soft_decoding {
        //     let input = sio.input(0).slice::<[LLR; MAX_SF]>();
        //     let output = sio.output(0).slice::<[LLR; MAX_SF]>();
        // No gray mapping , it has as been done directly in fft_demod block => block "bypass"
        output[0..nitems_to_process].copy_from_slice(&input[0..nitems_to_process]);
        // } else {
        //     let input = sio.input(0).slice::<u16>();
        //     let output = sio.output(0).slice::<u16>();
        //     output[0..nitems_to_process].copy_from_slice(
        //         &input[0..nitems_to_process]
        //             .iter()
        //             .map(|x| *x ^ (*x >> 1))
        //             .collect::<Vec<u16>>(), // Gray Demap
        //     );
        // }

        self.input.consume(nitems_to_process);
        self.output.produce(nitems_to_process);

        if self.input.finished() && nitems_to_process == input_len {
            io.finished = true;
        }

        Ok(())
    }
}
