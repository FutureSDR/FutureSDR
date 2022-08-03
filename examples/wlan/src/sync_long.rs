use futuresdr::anyhow::Result;
use futuresdr::async_trait::async_trait;
use futuresdr::log::debug;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::ItemTag;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::Tag;
use futuresdr::runtime::WorkIo;

const SEARCH_WINDOW: usize = 320;

#[derive(Debug)]
enum State {
    Broken,
    Sync(f32),
    Copy(f32, f32),
}

pub struct SyncLong {
    cor: [Complex32; SEARCH_WINDOW],
    cor_index: Vec<(usize, f32)>,
    state: State,
}

impl SyncLong {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("SyncLong").build(),
            StreamIoBuilder::new()
                .add_input("in", std::mem::size_of::<Complex32>())
                .add_output("out", std::mem::size_of::<Complex32>())
                .build(),
            MessageIoBuilder::new().build(),
            Self {
                cor: [Complex32::new(0.0, 0.0); SEARCH_WINDOW],
                cor_index: Vec::with_capacity(SEARCH_WINDOW),
                state: State::Broken,
            },
        )
    }

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

        self.cor_index = self.cor.iter().map(|x| x.norm_sqr()).enumerate().collect();
        self.cor_index.sort_by(|x, y| y.1.total_cmp(&x.1));

        println!("long top matches {:?}", &self.cor_index[0..5]);


        (320, 0.0)
    }
}

#[async_trait]
impl Kernel for SyncLong {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _m: &mut MessageIo<Self>,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<Complex32>();
        let out = sio.output(0).slice::<Complex32>();

        let mut m = std::cmp::min(input.len(), out.len());

        let tags = sio.input(0).tags();
        println!("long tags {:?}", &tags);
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
                debug!("new frame index {}  freq {}", index, freq);
                self.state = State::Sync(*freq);
            } else {
                m = std::cmp::min(m, *index);
            }
        }

        match self.state {
            State::Broken => {
                if m > 0 {
                    panic!("Sync Long is in broken state")
                }
            }
            State::Sync(freq_offset_short) => {
                if m >= SEARCH_WINDOW + 63 {
                    let (offset, freq_offset) = self.sync(&input[0..SEARCH_WINDOW + 63]);
                    sio.input(0).consume(offset);
                    io.call_again = true;

                    let freq_offset = 123.0;
                    self.state = State::Copy(freq_offset_short, freq_offset);
                }
            }
            State::Copy(freq_offset_from_short, freq_offset) => {
                sio.input(0).consume(m);
            }

        }

        if sio.input(0).finished() {
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
