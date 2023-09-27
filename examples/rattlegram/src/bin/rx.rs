use clap::Parser;
use futuresdr::anyhow::Result;
use futuresdr::blocks::audio::AudioSource;
use futuresdr::blocks::audio::FileSource;
use futuresdr::macros::async_trait;
use futuresdr::macros::connect;
use futuresdr::runtime::Block;
use futuresdr::runtime::BlockMeta;
use futuresdr::runtime::BlockMetaBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Kernel;
use futuresdr::runtime::MessageIo;
use futuresdr::runtime::MessageIoBuilder;
use futuresdr::runtime::Runtime;
use futuresdr::runtime::StreamIo;
use futuresdr::runtime::StreamIoBuilder;
use futuresdr::runtime::WorkIo;

use rattlegram::Decoder;
use rattlegram::DecoderResult;
use rattlegram::OperationMode;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(short, long)]
    file: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let mut fg = Flowgraph::new();

    let src = if let Some(f) = args.file {
        FileSource::new(&f)
    } else {
        #[cfg(debug_assertions)]
        println!("!!!PLEASE USE --release BUILD FOR LIVE DECODING!!!");
        AudioSource::new(48000, 1)
    };

    let snk = DecoderBlock::new();
    connect!(fg, src > snk);

    Runtime::new().run(fg)?;

    Ok(())
}

pub struct DecoderBlock {
    decoder: Box<Decoder>,
}

impl DecoderBlock {
    pub fn new() -> Block {
        Block::new(
            BlockMetaBuilder::new("RattegramDecoder").build(),
            StreamIoBuilder::new().add_input::<f32>("in").build(),
            MessageIoBuilder::new().build(),
            Self {
                decoder: Box::new(Decoder::new()),
            },
        )
    }
}

#[async_trait]
impl Kernel for DecoderBlock {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        sio: &mut StreamIo,
        _mio: &mut MessageIo<Self>,
        _meta: &mut BlockMeta,
    ) -> Result<()> {
        let input = sio.input(0).slice::<f32>();

        for s in input.chunks_exact(48000 / 50) {
            if !self.decoder.feed(s) {
                continue;
            }

            let status = self.decoder.process();
            let mut cfo = -1.0;
            let mut mode = OperationMode::Null;
            let mut call_sign = [0u8; 192];
            let mut payload = [0u8; 170];

            match status {
                DecoderResult::Okay => {}
                DecoderResult::Fail => {
                    println!("preamble fail");
                }
                DecoderResult::Sync => {
                    self.decoder.staged(&mut cfo, &mut mode, &mut call_sign);
                    println!("SYNC:");
                    println!("  CFO: {}", cfo);
                    println!("  Mode: {:?}", mode);
                    println!("  call sign: {}", String::from_utf8_lossy(&call_sign));
                }
                DecoderResult::Done => {
                    let flips = self.decoder.fetch(&mut payload);
                    println!("Bit flips: {}", flips);
                    println!("DONE: {}", String::from_utf8_lossy(&payload));
                }
                DecoderResult::Heap => {
                    println!("HEAP ERROR");
                }
                DecoderResult::Nope => {
                    self.decoder.staged(&mut cfo, &mut mode, &mut call_sign);
                    println!("NOPE:");
                    println!("  CFO: {}", cfo);
                    println!("  Mode: {:?}", mode);
                    println!("  call sign: {}", String::from_utf8_lossy(&call_sign));
                }
                DecoderResult::Ping => {
                    self.decoder.staged(&mut cfo, &mut mode, &mut call_sign);
                    println!("PING:");
                    println!("  CFO: {}", cfo);
                    println!("  Mode: {:?}", mode);
                    println!("  call sign: {}", String::from_utf8_lossy(&call_sign));
                }
                _ => {
                    panic!("wrong decoder result");
                }
            }
        }

        sio.input(0)
            .consume(input.len() / (48000 / 50) * (48000 / 50));
        if sio.input(0).finished() {
            io.finished = true;
        }

        Ok(())
    }
}
