use clap::Parser;
use futuresdr::anyhow::anyhow;
use futuresdr::anyhow::Result;
use std::fs::File;
use std::path::Path;

use rattlegram::Decoder;
use rattlegram::DecoderResult;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[clap(short, long, default_value = "out.wav")]
    file: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration: {args:?}");

    let mut inp_file = File::open(Path::new(&args.file))?;
    let (_, data) = wav::read(&mut inp_file)?;
    let samples = data.try_into_thirty_two_float();
    let mut samples = samples.or(Err(anyhow!("failed to convert")))?;

    let mut decoder = Decoder::new();

    let rate = 48000;
    let file_length = samples.len();
    let symbol_length = (1280 * rate) / 8000;
    let guard_length = symbol_length / 8;
    let extended_length = symbol_length + guard_length;
    let record_count = rate / 50;

    samples.extend_from_slice(&vec![0.0; 22 * record_count]);

    for s in samples.chunks(record_count) {
        if !decoder.feed(s) {
            continue;
        }

        let status = decoder.process();
        let cfo = -1.0;
        let mode = 0;
        let mut call_sign = [0u8;192];
        let mut payload = [0u8; 170];

        match status {
            DecoderResult::Okay => {
                break;
            }
            DecoderResult::Fail => {
                println!("preamble fail");
                break;
            }
            DecoderResult::Sync => {
                decoder.staged(&mut cfo, &mut mode, &mut call_sign);
                println!("SYNC:");
                println!("  CFO: {}", cfo);
                println!("  Mode: {}", mode);
                println!("  call sign: {}", String::from_utf8_lossy(&call_sign));
            }
            DecoderResult::Done => {
                let flips = decoder.fetch(&mut payload);
                println!("Bit flips: {}", flips);
                println!("DONE: {}", String::from_utf8_lossy(&payload));
            }
            DecoderResult::Heap => {
                println!("HEAP ERROR");
            }
            DecoderResult::Nope => {
                decoder.staged(&mut cfo, &mut mode, &mut call_sign);
                println!("NOPE:");
                println!("  CFO: {}", cfo);
                println!("  Mode: {}", mode);
                println!("  call sign: {}", String::from_utf8_lossy(&call_sign));
            }
            DecoderResult::Ping => {
                decoder.staged(&mut cfo, &mut mode, &mut call_sign);
                println!("PING:");
                println!("  CFO: {}", cfo);
                println!("  Mode: {}", mode);
                println!("  call sign: {}", String::from_utf8_lossy(&call_sign));
            }
            _ => {
                panic!("wrong decoder result");

            }
        }
	}
    Ok(())
}
