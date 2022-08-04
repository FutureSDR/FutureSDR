use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::num_complex::Complex32;
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

pub struct FrameEqualizer {
    freq_offset: f32,
    n_syms: usize,
    sym: [Complex32; 64],
}

impl FrameEqualizer {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("FrameEqualizer").build(),
            StreamIoBuilder::new()
                .add_input("in", std::mem::size_of::<Complex32>())
                .add_output("out", std::mem::size_of::<Complex32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                freq_offset: 0.0,
                n_syms: 0,
                sym: [Complex32::new(0.0, 0.0); 64],
            },
        )
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
        let out = sio.output(0).slice::<Complex32>();

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
                self.freq_offset = *freq;
                self.n_syms = 0;
            } else {
                input = &input[0..*index];
            }
        }

        let max_i = input.len() / 64 * 64;
        let max_o = out.len() / 48 * 48;
        let i = 0;
        let o = 0;

        for s in 0..max_i {
            // fft shift
            for k in 0..64 {
                let m = (k + 32) % 64;
                self.sym[m] = input[i * 64 + k];
            }
            let abs_sym = self.n_syms + s;

            if abs_sym == 0 {

            } else if abs_sym == 1 {

            } else {

            }
        }

        sio.input(0).consume(i * 64);
        sio.output(0).produce(o * 48);

        Ok(())
    }
}
