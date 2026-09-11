use futuresdr::runtime::dev::prelude::*;

const MIN_GAP: usize = 480;
const MAX_SAMPLES: usize = 540 * 80;
const NCO_RECALCULATE_INTERVAL: usize = 1024;
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
    pending_start_tag: Option<f32>,
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
            pending_start_tag: None,
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
        _mo: &mut MessageOutputs,
        _b: &BlockMeta,
    ) -> Result<()> {
        let in_sig = self.in_sig.slice();
        let in_abs = self.in_abs.slice();
        let in_cor = self.in_cor.slice();
        let in_cor_len = in_cor.len();
        let (out, mut tags) = self.output.slice_with_tags();

        let n_input = std::cmp::min(std::cmp::min(in_sig.len(), in_abs.len()), in_cor.len());

        let mut o = 0;
        let mut i = 0;
        let mut nco = None;

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
                        self.pending_start_tag = Some(f_offset);
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
                            self.pending_start_tag = Some(f_offset);
                            nco = None;
                            i += 1;
                            continue;
                        } else {
                            last_above_threshold = true;
                        }
                    } else {
                        last_above_threshold = false;
                    }

                    if n_copied == 0
                        && let Some(f_offset) = self.pending_start_tag.take()
                    {
                        tags.add_tag(o, Tag::NamedF32("wifi_start".to_string(), f_offset));
                    }

                    if nco.is_none() || n_copied.is_multiple_of(NCO_RECALCULATE_INTERVAL) {
                        nco = Some((
                            Complex32::from_polar(1.0, f_offset * n_copied as f32),
                            Complex32::from_polar(1.0, f_offset),
                        ));
                    }
                    let (phase, step) = nco.as_mut().unwrap();
                    out[o] = in_sig[i] * *phase;
                    *phase *= *step;
                    o += 1;

                    if n_copied + 1 == MAX_SAMPLES {
                        self.state = State::Search;
                        nco = None;
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

#[cfg(test)]
mod tests {
    use super::*;
    use futuresdr::runtime::mocker::Mocker;
    use futuresdr::runtime::mocker::Reader;
    use futuresdr::runtime::mocker::Writer;

    #[test]
    fn nco_matches_direct_frequency_correction() {
        const START: usize = 777;
        const LEN: usize = 2_048;
        const FREQUENCY: f32 = 0.0123;

        let mut block =
            SyncShort::<Reader<Complex32>, Reader<Complex32>, Reader<f32>, Writer<Complex32>>::new(
            );
        block.state = State::Copy(START, FREQUENCY, false);
        block.in_sig.set(vec![Complex32::new(0.25, -0.75); LEN]);
        block.in_abs.set(vec![Complex32::new(0.0, 0.0); LEN]);
        block.in_cor.set(vec![0.0; LEN]);
        block.output.reserve(LEN);

        let mut mocker = Mocker::new(block);
        mocker.run();
        let (output, _) = mocker.output.get();

        for (i, actual) in output.iter().enumerate() {
            let expected = Complex32::new(0.25, -0.75)
                * Complex32::from_polar(1.0, FREQUENCY * (START + i) as f32);
            assert!((actual - expected).norm() < 1.0e-4, "sample {i}");
        }
    }
}
