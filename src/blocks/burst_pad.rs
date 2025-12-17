use crate::prelude::*;

enum BurstPadState {
    Copy(usize, bool),
    PadHead(usize, usize, bool),
    PadTail(usize),
    HaveUntaggedSamples(usize),
    ClaimPending,
}

/// Pad head and/or tail of bursts by a fixed amount of samples, extending the burst tag.
///
/// # Stream Inputs
///
/// `in`: Input, expected to contain "burst_start" named_usize tags
///
/// # Stream Outputs
///
/// `out`: Output, potentially padded copy of the input samples
///
/// # Usage
/// ```rust
/// use futuresdr::blocks::{BurstPad, BurstSplit, BurstSizeRewriter};
/// // use futuresdr::blocks::seify::Builder;
/// use futuresdr::prelude::*;
/// /// produces a single tagged burst of 8 zeroes
/// #[derive(Block)]
/// struct DummyBurstSource<
///     O = DefaultCpuWriter<Complex32>,
/// >
/// where
///     O: CpuBufferWriter<Item = Complex32>,
/// {
///     #[output]
///     output: O,
/// }
/// impl<O> DummyBurstSource<O>
/// where
///     O: CpuBufferWriter<Item = Complex32>,
/// {
///     pub fn new() -> Self {
///         let mut out = O::default();
///         out.set_min_items(8);
///         Self{
///             output: out,
///         }
///     }
/// }
/// impl<O> Kernel for DummyBurstSource<O>
/// where
///     O: CpuBufferWriter<Item = Complex32>,
/// {
///     async fn work(
///         &mut self,
///         io: &mut WorkIo,
///         _m: &mut MessageOutputs,
///         _b: &mut BlockMeta,
///     ) -> Result<()> {
///         let (out, mut out_tags) = self.output.slice_with_tags();
///         if out.len() >= 8 {
///             out[..8].fill(Complex32::new(0.0, 0.0));
///             out_tags.add_tag(0, Tag::NamedUsize("burst_start".to_string(), 8));
///             self.output.produce(8);
///             io.finished = true;
///         }
///         Ok(())
///     }
/// }
/// // example pipeline to enable burst transmissions with the HackRF One in Half-Duplex mode:
/// // first, extend each burst for individual lossless transmission, as the hackrf might discard queued samples when switching back to receive mode
/// // then, pad the bursts to multiples of the hackrf transmit buffer size to ensure immediate transmission without the sink waiting to fill the buffer first
/// // finally, break long bursts up into sub-bursts no longer than the hackrf transmit buffer size, as longer burst transmissions are currently not supported by the driver.
/// // Overall, this ensures immediate and lossless transmission of bursts. While there is a chance for buffer underflow corrupting frames due to the chunking in the last step, this is very small, as the previous padding and a sufficiently sized buffer at the sink ensure that the required amount of samples to finish the original burst shoud be pretty much immediately available.
/// fn main() -> Result<()> {
///     // get a sample stream that contains "burst_start" tags
///     let source: DummyBurstSource = DummyBurstSource::new();
///     // keep track of the maximum expected burst size
///     let mut max_burst_size: usize = 8;
///     // pad each individual burst (there is only one in this example) to ensure lossless transmission with the HackRF One in Half-Duplex mode
///     let pad_head: BurstPad = BurstPad::new_for_hackrf();
///     // keep track of the maximum expected burst size
///     max_burst_size = pad_head.propagate_max_burst_size(max_burst_size);
///     // create sink with sufficiently sized input buffer to not stall on large bursts
///     // let sink = Builder::new("driver=soapy,soapy_driver=hackrf")?
///     //        .min_in_buffer_size(max_burst_size) // make sure the sink won't deadlock on large bursts due to insufficient buffer size
///     //        .build_sink()?;
///     // [connect and run Flowgraph]
///     Ok(())
/// }
/// ```
#[derive(Block)]
pub struct BurstPad<
    T: Copy + Send + 'static = Complex32,
    I = DefaultCpuReader<T>,
    O = DefaultCpuWriter<T>,
> where
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    state: BurstPadState,
    num_samples_pad_head: usize,
    num_samples_pad_tail: usize,
    pad_value: T,
}

impl<T: Copy + Send + 'static, I, O> BurstPad<T, I, O>
where
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    /// Create [`struct@futuresdr::blocks::CopyAndTag`] block
    pub fn new(num_samples_head: usize, num_samples_tail: usize, value: T) -> Self {
        BurstPad::<T, I, O> {
            input: I::default(),
            output: O::default(),
            state: BurstPadState::ClaimPending,
            num_samples_pad_head: num_samples_head,
            num_samples_pad_tail: num_samples_tail,
            pad_value: value,
        }
    }

    /// compute the maximum produced burst length based on the maximum expected input sample burst.
    /// useful for correctly sizing the input buffer of a burst-aware downstream sink to avoid deadlocks, as it needs to wait for the whole burst to be available before starting to precess it.
    pub fn propagate_max_burst_size(&self, max_input_burst_size: usize) -> usize {
        self.num_samples_pad_head + max_input_burst_size + self.num_samples_pad_tail
    }
}

impl<T: Copy + Send + 'static, I, O> Kernel for BurstPad<T, I, O>
where
    I: CpuBufferReader<Item = T>,
    O: CpuBufferWriter<Item = T>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let mut consumed = 0;
        let mut produced = 0;

        let (i, i_tags) = self.input.slice_with_tags();
        let (o, mut o_tags) = self.output.slice_with_tags();
        let num_samples_in_input = i.len();
        let num_slots_in_output = o.len();

        'outer: loop {
            match self.state {
                BurstPadState::Copy(num_samples_left, tag_pending) => {
                    let n_consume = (num_samples_in_input - consumed).min(num_samples_left);
                    let n_produce = (num_slots_in_output - produced).min(num_samples_left);
                    let n_copy = n_consume.min(n_produce);
                    if n_copy > 0 {
                        if tag_pending {
                            o_tags.add_tag(
                                produced,
                                Tag::NamedUsize(
                                    "burst_start".to_string(),
                                    num_samples_left + self.num_samples_pad_tail,
                                ),
                            );
                        }
                        o[produced..produced + n_copy]
                            .copy_from_slice(&i[consumed..consumed + n_copy]);
                        consumed += n_copy;
                        produced += n_copy;
                        if n_copy == num_samples_left {
                            if self.num_samples_pad_tail > 0 {
                                self.state = BurstPadState::PadTail(self.num_samples_pad_tail);
                            } else {
                                self.state = BurstPadState::ClaimPending;
                            }
                        } else {
                            self.state = BurstPadState::Copy(num_samples_left - n_copy, false);
                        }
                    } else {
                        break 'outer;
                    }
                }
                BurstPadState::PadHead(num_samples_left, original_burst_len, tag_pending) => {
                    let n_produce = (num_slots_in_output - produced).min(num_samples_left);
                    if n_produce > 0 {
                        if tag_pending {
                            o_tags.add_tag(
                                produced,
                                Tag::NamedUsize(
                                    "burst_start".to_string(),
                                    num_samples_left
                                        + original_burst_len
                                        + self.num_samples_pad_tail,
                                ),
                            );
                        }
                        for slot in o.iter_mut().skip(produced).take(n_produce) {
                            *slot = self.pad_value;
                        }
                        produced += n_produce;
                        if n_produce == num_samples_left {
                            self.state = BurstPadState::Copy(original_burst_len, false);
                        } else {
                            self.state = BurstPadState::PadHead(
                                num_samples_left - n_produce,
                                original_burst_len,
                                false,
                            );
                        }
                    } else {
                        break 'outer;
                    }
                }
                BurstPadState::PadTail(num_samples_left) => {
                    let n_produce = (num_slots_in_output - produced).min(num_samples_left);
                    if n_produce > 0 {
                        for slot in o.iter_mut().skip(produced).take(n_produce) {
                            *slot = self.pad_value;
                        }
                        produced += n_produce;
                        if n_produce == num_samples_left {
                            self.state = BurstPadState::ClaimPending;
                        } else {
                            self.state = BurstPadState::PadTail(num_samples_left - n_produce);
                        }
                    } else {
                        break 'outer;
                    }
                }
                BurstPadState::HaveUntaggedSamples(num_samples_left) => {
                    self.state = BurstPadState::Copy(num_samples_left, false);
                    // TODO this just copies
                }
                BurstPadState::ClaimPending => {
                    // get new state depending on available input
                    if num_samples_in_input - consumed == 0 {
                        break 'outer;
                    } else {
                        self.state = i_tags
                            .iter()
                            .find_map(|x| match x {
                                ItemTag {
                                    index,
                                    tag: Tag::NamedUsize(n, len),
                                } => {
                                    if n == "burst_start" {
                                        if *index < consumed {
                                            warn!("dropping missed Tag: Tag::NamedUsize({n}, {len}) @ index {index}.");
                                            None
                                        } else if *index == consumed {
                                            if self.num_samples_pad_head > 0 {
                                                Some(BurstPadState::PadHead(
                                                    self.num_samples_pad_head,
                                                    *len,
                                                    true,
                                                ))
                                            } else {
                                                Some(BurstPadState::Copy(*len, true))
                                            }
                                        } else {
                                            Some(BurstPadState::HaveUntaggedSamples(*index - consumed))
                                        }
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            })
                            .unwrap_or(BurstPadState::HaveUntaggedSamples(
                                num_samples_in_input - consumed,
                            ));
                    }
                }
            }
        }

        if consumed > 0 {
            // debug!("consumed {consumed} samples");
            self.input.consume(consumed);
        }
        if produced > 0 {
            // debug!("produced {produced} samples");
            self.output.produce(produced);
        }

        if self.input.finished() && consumed == num_samples_in_input {
            io.finished = true;
        };

        Ok(())
    }
}
