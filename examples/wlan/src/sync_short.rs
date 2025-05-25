use futuresdr::prelude::*;

const MIN_GAP: usize = 480;
const MAX_SAMPLES: usize = 540 * 80;
const THRESHOLD: f32 = 0.56;

#[derive(Debug)]
enum State {
    Search,
    Found,
    Copy(usize, f32, bool),
}

#[derive(Block)]
pub struct SyncShort<
    I0 = DefaultCpuReader<Complex32>,
    I1 = DefaultCpuReader<Complex32>,
    I2 = DefaultCpuReader<f32>,
    O = DefaultCpuWriter<Complex32>,
> where
    I0: CpuBufferReader<Item = Complex32>,
    I1: CpuBufferReader<Item = Complex32>,
    I2: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    #[input]
    in_sig: I0,
    #[input]
    in_abs: I1,
    #[input]
    in_cor: I2,
    #[output]
    output: O,
    state: State,
}

impl<I0, I1, I2, O> SyncShort<I0, I1, I2, O>
where
    I0: CpuBufferReader<Item = Complex32>,
    I1: CpuBufferReader<Item = Complex32>,
    I2: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    pub fn new() -> Self {
        Self {
            in_sig: I0::default(),
            in_abs: I1::default(),
            in_cor: I2::default(),
            output: O::default(),
            state: State::Search,
        }
    }
}
impl<I0, I1, I2, O> Default for SyncShort<I0, I1, I2, O>
where
    I0: CpuBufferReader<Item = Complex32>,
    I1: CpuBufferReader<Item = Complex32>,
    I2: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<I0, I1, I2, O> Kernel for SyncShort<I0, I1, I2, O>
where
    I0: CpuBufferReader<Item = Complex32>,
    I1: CpuBufferReader<Item = Complex32>,
    I2: CpuBufferReader<Item = f32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let in_sig = self.in_sig.slice();
        let in_abs = self.in_abs.slice();
        let in_cor = self.in_cor.slice();
        let in_cor_len = in_cor.len();
        let (out, mut tags) = self.output.slice_with_tags();

        let n_input = std::cmp::min(std::cmp::min(in_sig.len(), in_abs.len()), in_cor.len());

        let mut o = 0;
        let mut i = 0;

        while i < n_input && o < out.len() {
            match self.state {
                State::Search => {
                    if in_cor[i] > THRESHOLD {
                        self.state = State::Found;
                    }
                }
                State::Found => {
                    if in_cor[i] > THRESHOLD {
                        let f_offset = -in_abs[i].arg() / 16.0;
                        self.state = State::Copy(0, f_offset, false);
                        tags.add_tag(o, Tag::NamedF32("wifi_start".to_string(), f_offset));
                    } else {
                        self.state = State::Search;
                    }
                }
                State::Copy(n_copied, f_offset, mut last_above_threshold) => {
                    if in_cor[i] > THRESHOLD {
                        // resync
                        if last_above_threshold && n_copied > MIN_GAP {
                            let f_offset = -in_abs[i].arg() / 16.0;
                            self.state = State::Copy(0, f_offset, false);
                            tags.add_tag(o, Tag::NamedF32("wifi_start".to_string(), f_offset));
                            i += 1;
                            continue;
                        } else {
                            last_above_threshold = true;
                        }
                    } else {
                        last_above_threshold = false;
                    }

                    out[o] = in_sig[i] * Complex32::from_polar(1.0, f_offset * n_copied as f32); // accum?
                    o += 1;

                    if n_copied + 1 == MAX_SAMPLES {
                        self.state = State::Search;
                    } else {
                        self.state = State::Copy(n_copied + 1, f_offset, last_above_threshold);
                    }
                }
            }
            i += 1;
        }

        self.in_sig.consume(i);
        self.in_abs.consume(i);
        self.in_cor.consume(i);
        self.output.produce(o);

        if self.in_cor.finished() && i == in_cor_len {
            io.finished = true;
        }

        Ok(())
    }
}
