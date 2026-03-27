use anyhow::ensure;
use clap::Parser;
use clap::ValueEnum;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Delay;
use futuresdr::blocks::Fft;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::scheduler::FlowScheduler;
use std::time;

use wlan::Decoder;
use wlan::FrameEqualizer;
use wlan::MovingAverage;
use wlan::SyncLong;
use wlan::SyncShort;

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Config {
    Normal,
    Opti,
}

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Run number
    #[clap(long, default_value_t = 0)]
    run: usize,
    /// File name
    #[clap(short, long, default_value = "wlan-100.cf32")]
    file: String,
    /// Runtime config
    #[clap(long, value_enum, default_value_t = Config::Normal)]
    config: Config,
}

fn load_cf32(path: &str) -> Result<Vec<Complex32>> {
    let bytes = std::fs::read(path)?;
    ensure!(
        bytes.len() % 8 == 0,
        "invalid cf32 file size ({}), expected multiple of 8 bytes",
        bytes.len()
    );

    let mut out = Vec::with_capacity(bytes.len() / 8);
    for chunk in bytes.chunks_exact(8) {
        let re = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let im = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        out.push(Complex32::new(re, im));
    }
    Ok(out)
}

fn normal(args: Args, samples: Vec<Complex32>) -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = VectorSource::<Complex32>::new(samples);
    let delay = Delay::<Complex32>::new(16);
    let complex_to_mag_2 = Apply::<_, _, _>::new(|i: &Complex32| i.norm_sqr());
    let float_avg = MovingAverage::<f32>::new(64);
    let mult_conj = Combine::<_, _, _, _>::new(|a: &Complex32, b: &Complex32| a * b.conj());
    let complex_avg = MovingAverage::<Complex32>::new(48);
    let divide_mag = Combine::<_, _, _, _>::new(|a: &Complex32, b: &f32| a.norm() / b);
    let sync_short: SyncShort = SyncShort::new();
    let sync_long: SyncLong = SyncLong::new();
    let fft: Fft = Fft::new(64);
    let frame_equalizer: FrameEqualizer = FrameEqualizer::new();
    let decoder = Decoder::new();

    connect!(fg, src > delay;
        src > complex_to_mag_2 > float_avg;
        src > in0.mult_conj > complex_avg;
        delay > in_sig.sync_short;
        complex_avg > in_abs.sync_short;
        divide_mag > in_cor.sync_short;
        delay > in1.mult_conj;
        complex_avg > in0.divide_mag; float_avg > in1.divide_mag;
        sync_short > sync_long > fft > frame_equalizer > decoder);

    let now = time::Instant::now();
    Runtime::new().run(fg)?;
    let elapsed = now.elapsed();

    println!(
        "{},{},normal,{}",
        args.run,
        args.file,
        elapsed.as_secs_f64()
    );

    Ok(())
}

fn opti(args: Args, samples: Vec<Complex32>) -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = VectorSource::<Complex32>::new(samples);
    let delay = Delay::<Complex32>::new(16);
    let complex_to_mag_2 = Apply::<_, _, _>::new(|i: &Complex32| i.norm_sqr());
    let float_avg = MovingAverage::<f32>::new(64);
    let mult_conj = Combine::<_, _, _, _>::new(|a: &Complex32, b: &Complex32| a * b.conj());
    let complex_avg = MovingAverage::<Complex32>::new(48);
    let divide_mag = Combine::<_, _, _, _>::new(|a: &Complex32, b: &f32| a.norm() / b);
    let sync_short: SyncShort = SyncShort::new();
    let sync_long: SyncLong = SyncLong::new();
    let fft: Fft = Fft::new(64);
    let frame_equalizer: FrameEqualizer = FrameEqualizer::new();
    let decoder = Decoder::new();

    connect!(fg, src > delay;
        src > complex_to_mag_2 > float_avg;
        src > in0.mult_conj > complex_avg;
        delay > in_sig.sync_short;
        complex_avg > in_abs.sync_short;
        divide_mag > in_cor.sync_short;
        delay > in1.mult_conj;
        complex_avg > in0.divide_mag; float_avg > in1.divide_mag;
        sync_short > sync_long > fft > frame_equalizer > decoder);

    let block_mapping: Vec<Vec<BlockId>> = vec![
        vec![
            src.into(),
            delay.into(),
            mult_conj.into(),
            complex_to_mag_2.into(),
        ],
        vec![
            complex_avg.into(),
            float_avg.into(),
            divide_mag.into(),
            sync_short.into(),
        ],
        vec![
            sync_long.into(),
            fft.into(),
            frame_equalizer.into(),
            decoder.into(),
        ],
        vec![],
    ];

    let now = time::Instant::now();
    Runtime::with_scheduler(FlowScheduler::with_pinned_blocks(block_mapping)).run(fg)?;
    let elapsed = now.elapsed();

    println!("{},{},opti,{}", args.run, args.file, elapsed.as_secs_f64());

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let samples = load_cf32(&args.file)?;
    let n = samples.len();
    let samples: Vec<Complex32> = samples.into_iter().cycle().take(n * 10).collect();

    match args.config {
        Config::Normal => normal(args, samples)?,
        Config::Opti => opti(args, samples)?,
    }

    Ok(())
}
