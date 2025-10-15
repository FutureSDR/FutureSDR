use futuresdr::prelude::*;

const SEARCH_WINDOW: usize = 320;

#[derive(Debug)]
enum State {
    Broken,
    Sync(f32),
    Copy(usize, f32),
}

struct Correlator {
    cor: [Complex32; SEARCH_WINDOW],
    cor_index: Vec<(usize, f32)>,
}

impl Correlator {
    fn sync(&mut self, input: &[Complex32]) -> (usize, f32) {
        debug_assert_eq!(input.len(), SEARCH_WINDOW + 63);

        for i in 0..SEARCH_WINDOW {
            unsafe {
                let mut sum = Complex32::new(0.0, 0.0);
                for k in 0..64 {
                    sum += *input.get_unchecked(i + k) * *LONG.get_unchecked(k);
                }
                *self.cor.get_unchecked_mut(i) = sum;
            }
        }

        // let mut foo : Vec<(usize, Complex32)> = self.cor.iter().copied().enumerate().collect();
        // foo.sort_by(|x, y| y.1.norm().total_cmp(&x.1.norm()));
        // println!("top {:?}", &foo[0..5]);

        self.cor_index = self.cor.iter().map(|x| x.norm_sqr()).enumerate().collect();
        self.cor_index.sort_by(|x, y| y.1.total_cmp(&x.1));
        let (first, second) = if self.cor_index[0].0 < self.cor_index[1].0 {
            (self.cor_index[0].0, self.cor_index[1].0)
        } else {
            (self.cor_index[1].0, self.cor_index[0].0)
        };

        (
            first,
            (self.cor[first] * self.cor[second].conj()).arg() / 64.0,
        )
    }
}

#[derive(Block)]
pub struct SyncLong<I = DefaultCpuReader<Complex32>, O = DefaultCpuWriter<Complex32>>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    #[input]
    input: I,
    #[output]
    output: O,
    corr: Correlator,
    state: State,
}

impl<I, O> SyncLong<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    pub fn new() -> Self {
        Self {
            input: I::default(),
            output: O::default(),
            corr: Correlator {
                cor: [Complex32::new(0.0, 0.0); SEARCH_WINDOW],
                cor_index: Vec::with_capacity(SEARCH_WINDOW),
            },
            state: State::Broken,
        }
    }
}

impl<I, O> Default for SyncLong<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<I, O> Kernel for SyncLong<I, O>
where
    I: CpuBufferReader<Item = Complex32>,
    O: CpuBufferWriter<Item = Complex32>,
{
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let (input, in_tags) = self.input.slice_with_tags();
        let input_len = input.len();
        let (out, mut out_tags) = self.output.slice_with_tags();

        let mut m = std::cmp::min(input.len(), out.len());

        // println!("long tags {:?}", &tags);
        if let Some((index, freq)) = in_tags.iter().find_map(|x| match x {
            ItemTag {
                index,
                tag: Tag::NamedF32(n, f),
            } => {
                if n == "wifi_start" {
                    Some((index, f))
                } else {
                    None
                }
            }
            _ => None,
        }) {
            if *index == 0 {
                self.state = State::Sync(*freq);
            } else {
                m = std::cmp::min(m, *index);
                if m < 80 {
                    self.input.consume(m);
                    return Ok(());
                }
            }
        }

        match self.state {
            State::Broken => {
                if m > 0 {
                    panic!("Sync Long is in broken state")
                }
            }
            State::Sync(freq_offset_short) => {
                if m >= SEARCH_WINDOW + 128 {
                    let (offset, freq_offset) = self.corr.sync(&input[0..SEARCH_WINDOW + 63]);
                    // debug!("long start: offset {}   freq {}", offset, freq_offset);

                    for i in 0..128 {
                        out[i] =
                            input[offset + i] * Complex32::from_polar(1.0, i as f32 * freq_offset);
                    }
                    out_tags.add_tag(
                        0,
                        Tag::NamedF32("wifi_start".to_string(), freq_offset_short + freq_offset),
                    );

                    self.input.consume(offset + 128);
                    self.output.produce(128);
                    io.call_again = true;

                    self.state = State::Copy(0, freq_offset);
                }
            }
            State::Copy(n_copied, freq_offset) => {
                let syms = m / 80;
                for i in 0..syms {
                    for k in 0..64 {
                        out[i * 64 + k] = input[i * 80 + 16 + k]
                            * Complex32::from_polar(
                                1.0,
                                ((n_copied + i) * 80 + 128 + 16 + k) as f32 * freq_offset,
                            );
                    }
                }
                self.input.consume(syms * 80);
                self.output.produce(syms * 64);
                self.state = State::Copy(n_copied + syms * 80, freq_offset);
            }
        }

        if self.input.finished() && input_len - m < 80 {
            io.finished = true;
        }

        Ok(())
    }
}

const LONG: [Complex32; 64] = [
    Complex32::new(1.3868, -0.0000),
    Complex32::new(-0.0455, 1.0679),
    Complex32::new(0.3528, 0.9865),
    Complex32::new(0.8594, -0.7348),
    Complex32::new(0.1874, -0.2475),
    Complex32::new(0.5309, 0.7784),
    Complex32::new(-1.0218, 0.4897),
    Complex32::new(-0.3401, 0.9423),
    Complex32::new(0.8657, 0.2298),
    Complex32::new(0.4734, -0.0362),
    Complex32::new(0.0088, 1.0207),
    Complex32::new(-1.2142, 0.4205),
    Complex32::new(0.2172, 0.5195),
    Complex32::new(0.5207, 0.1326),
    Complex32::new(-0.1995, -1.4259),
    Complex32::new(1.0583, 0.0363),
    Complex32::new(0.5547, 0.5547),
    Complex32::new(0.3277, -0.8728),
    Complex32::new(-0.5077, -0.3488),
    Complex32::new(-1.1650, -0.5789),
    Complex32::new(0.7297, -0.8197),
    Complex32::new(0.6173, -0.1253),
    Complex32::new(-0.5353, -0.7214),
    Complex32::new(-0.5011, 0.1935),
    Complex32::new(-0.3110, 1.3392),
    Complex32::new(-1.0818, 0.1470),
    Complex32::new(-1.1300, 0.1820),
    Complex32::new(0.6663, 0.6571),
    Complex32::new(-0.0249, -0.4773),
    Complex32::new(-0.8155, -1.0218),
    Complex32::new(0.8140, -0.9396),
    Complex32::new(0.1090, -0.8662),
    Complex32::new(-1.3868, -0.0000),
    Complex32::new(0.1090, 0.8662),
    Complex32::new(0.8140, 0.9396),
    Complex32::new(-0.8155, 1.0218),
    Complex32::new(-0.0249, 0.4773),
    Complex32::new(0.6663, -0.6571),
    Complex32::new(-1.1300, -0.1820),
    Complex32::new(-1.0818, -0.1470),
    Complex32::new(-0.3110, -1.3392),
    Complex32::new(-0.5011, -0.1935),
    Complex32::new(-0.5353, 0.7214),
    Complex32::new(0.6173, 0.1253),
    Complex32::new(0.7297, 0.8197),
    Complex32::new(-1.1650, 0.5789),
    Complex32::new(-0.5077, 0.3488),
    Complex32::new(0.3277, 0.8728),
    Complex32::new(0.5547, -0.5547),
    Complex32::new(1.0583, -0.0363),
    Complex32::new(-0.1995, 1.4259),
    Complex32::new(0.5207, -0.1326),
    Complex32::new(0.2172, -0.5195),
    Complex32::new(-1.2142, -0.4205),
    Complex32::new(0.0088, -1.0207),
    Complex32::new(0.4734, 0.0362),
    Complex32::new(0.8657, -0.2298),
    Complex32::new(-0.3401, -0.9423),
    Complex32::new(-1.0218, -0.4897),
    Complex32::new(0.5309, -0.7784),
    Complex32::new(0.1874, 0.2475),
    Complex32::new(0.8594, 0.7348),
    Complex32::new(0.3528, -0.9865),
    Complex32::new(-0.0455, -1.0679),
];
