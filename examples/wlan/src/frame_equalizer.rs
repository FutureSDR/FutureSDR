use crate::Modulation;
use crate::FrameParam;
use crate::Mcs;

use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::num_complex::Complex32;
use futuresdr::log::info;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::Tag;

struct Equalizer {

}

impl Equalizer {
    fn new() -> Self {
        Equalizer {}
    }
    fn sync1(&mut self, s: &[Complex32; 64]) {
        
    }
    fn sync2(&mut self, s: &[Complex32; 64]) {
        
    }
    fn equalize(&mut self, input: &[Complex32; 64], output: &mut [u8; 48], modulation: &Modulation) {
        
    }
}

enum State {
    Sync1,
    Sync2,
    Signal,
    Copy(usize, Modulation),
    Skip,
}

pub struct FrameEqualizer {
    equalizer: Equalizer,
    state: State,
    sym_in: [Complex32; 64],
    sym_out: [u8; 48],
}

impl FrameEqualizer {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("FrameEqualizer").build(),
            StreamIoBuilder::new()
                .add_input("in", std::mem::size_of::<Complex32>())
                .add_output("out", std::mem::size_of::<u8>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                equalizer: Equalizer::new(),
                state: State::Skip,
                sym_in: [Complex32::new(0.0, 0.0); 64],
                sym_out: [0; 48],
            },
        )
    }

    fn decode_signal_field(syms: &[u8; 48]) -> Option<FrameParam> {
        None
    }
}

#[async_trait]
impl Kernel for FrameEqualizer {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let mut input = sio.input(0).slice::<Complex32>();
        let out = sio.output(0).slice::<u8>();

        let tags = sio.input(0).tags();
        if let Some((index, freq)) = tags.iter().find_map(|x| match x {
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
                if !matches!(self.state, State::Skip) {
                    info!("frame equalizer: canceling frame");
                }
                self.state = State::Sync1;
            } else {
                input = &input[0..*index];
            }
        }

        let max_i = input.len() / 64;
        let max_o = out.len() / 48;
        let mut i = 0;
        let mut o = 0;

        while i < max_i {
            // copy symbol w/ fft shift
            for k in 0..64 {
                let m = (k + 32) % 64;
                self.sym_in[m] = input[i * 64 + k];
            }

            match &mut self.state {
                State::Sync1 => {
                    self.equalizer.sync1(&self.sym_in);
                    self.state = State::Sync2;
                    i += 1;
                }
                State::Sync2 => {
                    self.equalizer.sync2(&self.sym_in);
                    self.state = State::Signal;
                    i += 1;
                }
                State::Signal => {
                    self.equalizer.equalize(&self.sym_in, &mut self.sym_out, &Modulation::Bpsk);
                    i += 1;
                    if let Some(frame) = Self::decode_signal_field(&self.sym_out) {
                        sio.output(0).add_tag(o * 48, Tag::Id(123));
                        self.state = State::Copy(frame.symbols(), frame.modulation()); 
                    } else {
                        self.state = State::Skip; 
                    }
                }
                State::Copy(mut n_sym, modulation) => {
                    if o < max_o {
                        i += 1;
                        o += 1;

                        self.equalizer.equalize(&self.sym_in, &mut out[o*48..(o+1)*48].try_into().unwrap(), modulation);

                        n_sym -= 1;
                        if n_sym == 0 {
                            self.state = State::Skip;
                        }
                    } else {
                        break;
                    }
                }
                State::Skip => {
                    i += 1;
                }
            }
        }

        sio.input(0).consume(i * 64);
        sio.output(0).produce(o * 48);

        Ok(())
    }
}
