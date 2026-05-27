use clap::Parser;
use clap::ValueEnum;
use futuresdr::blocks::Apply;
use futuresdr::blocks::Combine;
use futuresdr::blocks::Delay;
use futuresdr::blocks::Fft;
use futuresdr::blocks::FileSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::circular;
use perf::local_mpsc;
use perf::local_spsc;
use perf::local_spsc_tags;
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
    /// FutureSDR buffer size in bytes
    #[clap(long, default_value_t = 262_144)]
    buffer_size: i64,
}

// fn load_cf32(path: &str) -> Result<Vec<Complex32>> {
//     use anyhow::ensure;
//     let bytes = std::fs::read(path)?;
//     ensure!(
//         bytes.len() % 8 == 0,
//         "invalid cf32 file size ({}), expected multiple of 8 bytes",
//         bytes.len()
//     );
//
//     let mut out = Vec::with_capacity(bytes.len() / 8);
//     for chunk in bytes.chunks_exact(8) {
//         let re = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
//         let im = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
//         out.push(Complex32::new(re, im));
//     }
//     Ok(out)
// }

fn normal(args: Args) -> Result<()> {
    let mut fg = Flowgraph::new();

    let src = FileSource::<Complex32>::new(&args.file, false);
    let delay = Delay::<Complex32>::new(16);
    let complex_to_mag_2 = Apply::new(|i: &Complex32| i.norm_sqr());
    let float_avg = MovingAverage::<f32>::new(64);
    let mult_conj = Combine::new(|a: &Complex32, b: &Complex32| a * b.conj());
    let complex_avg = MovingAverage::<Complex32>::new(48);
    let divide_mag = Combine::new(|a: &Complex32, b: &f32| a.norm() / b);
    let sync_short: SyncShort = SyncShort::new();
    let sync_long: SyncLong = SyncLong::new();
    let fft = Fft::new(64);
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

    let runtime = Runtime::new();
    let now = time::Instant::now();
    runtime.run(fg)?;
    let elapsed = now.elapsed();

    println!(
        "{},{},normal,{}",
        args.run,
        args.file,
        elapsed.as_secs_f64()
    );

    Ok(())
}

fn local_domains(fg: &mut Flowgraph, n: usize) -> Result<Vec<LocalDomain>> {
    let cores = core_affinity::get_core_ids().unwrap_or_default();
    let mut domains = Vec::with_capacity(n);
    if cores.len() >= n {
        for core in cores.into_iter().take(n) {
            domains.push(fg.local_domain_pinned(core.id)?);
        }
    } else {
        for _ in 0..n {
            domains.push(fg.local_domain()?);
        }
    }
    Ok(domains)
}

fn opti(args: Args) -> Result<()> {
    type CircularComplexReader = circular::Reader<Complex32>;
    type CircularComplexWriter = circular::Writer<Complex32>;
    type LocalMpscComplexReader = local_mpsc::Reader<Complex32>;
    type LocalMpscComplexWriter = local_mpsc::Writer<Complex32>;
    type LocalSpscComplexReader = local_spsc::Reader<Complex32>;
    type LocalSpscComplexWriter = local_spsc::Writer<Complex32>;
    type LocalSpscF32Reader = local_spsc::Reader<f32>;
    type LocalSpscF32Writer = local_spsc::Writer<f32>;
    type LocalSpscTagsComplexReader = local_spsc_tags::Reader<Complex32>;
    type LocalSpscTagsComplexWriter = local_spsc_tags::Writer<Complex32>;
    type LocalSpscTagsU8Reader = local_spsc_tags::Reader<u8>;
    type LocalSpscTagsU8Writer = local_spsc_tags::Writer<u8>;

    let mut fg = Flowgraph::new();

    let domains = local_domains(&mut fg, 2)?;
    let [local0, local1]: [LocalDomain; 2] = domains
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected two local domains"))?;

    let file = args.file.clone();
    let src = fg.add_local(local0, move || {
        FileSource::<Complex32, LocalMpscComplexWriter>::new(&file, false)
    });
    let delay = fg.add_local(local0, || {
        Delay::<Complex32, LocalMpscComplexReader, LocalMpscComplexWriter>::new(16)
    });
    let complex_to_mag_2 = fg.add_local(local0, || {
        Apply::<_, _, _, LocalMpscComplexReader, LocalSpscF32Writer>::with_buffers(
            |i: &Complex32| i.norm_sqr(),
        )
    });
    let float_avg = fg.add_local(local0, || {
        MovingAverage::<f32, LocalSpscF32Reader, LocalSpscF32Writer>::new(64)
    });
    let mult_conj = fg.add_local(local0, || {
        Combine::<
            _,
            _,
            _,
            _,
            LocalMpscComplexReader,
            LocalMpscComplexReader,
            LocalSpscComplexWriter,
        >::with_buffers(|a: &Complex32, b: &Complex32| a * b.conj())
    });
    let complex_avg = fg.add_local(local0, || {
        MovingAverage::<Complex32, LocalSpscComplexReader, LocalMpscComplexWriter>::new(48)
    });
    let divide_mag = fg.add_local(local0, || {
        Combine::<_, _, _, _, LocalMpscComplexReader, LocalSpscF32Reader, LocalSpscF32Writer>::with_buffers(
            |a: &Complex32, b: &f32| a.norm() / b,
        )
    });
    let sync_short = fg.add_local(local0, || {
        SyncShort::<
            LocalMpscComplexReader,
            LocalMpscComplexReader,
            LocalSpscF32Reader,
            CircularComplexWriter,
        >::new()
    });

    let sync_long = fg.add_local(local1, || {
        SyncLong::<CircularComplexReader, LocalSpscTagsComplexWriter>::new()
    });
    let fft = fg.add_local(local1, || {
        Fft::<LocalSpscTagsComplexReader, LocalSpscTagsComplexWriter>::with_buffers(64)
    });
    let frame_equalizer = fg.add_local(local1, || {
        FrameEqualizer::<LocalSpscTagsComplexReader, LocalSpscTagsU8Writer>::new()
    });
    let decoder = fg.add_local(local1, || Decoder::<LocalSpscTagsU8Reader>::new());

    connect!(fg, src ~> delay;
        src ~> complex_to_mag_2 ~> float_avg;
        src ~> in0.mult_conj ~> complex_avg;
        delay ~> in_sig.sync_short;
        complex_avg ~> in_abs.sync_short;
        divide_mag ~> in_cor.sync_short;
        delay ~> in1.mult_conj;
        complex_avg ~> in0.divide_mag; float_avg ~> in1.divide_mag;
        sync_short > sync_long ~> fft ~> frame_equalizer ~> decoder);

    let runtime = Runtime::new();
    let now = time::Instant::now();
    runtime.run(fg)?;
    let elapsed = now.elapsed();

    println!("{},{},opti,{}", args.run, args.file, elapsed.as_secs_f64());

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    futuresdr::runtime::config::set("ctrlport_enable", false);
    futuresdr::runtime::config::set("buffer_size", args.buffer_size);

    match args.config {
        Config::Normal => normal(args)?,
        Config::Opti => opti(args)?,
    }

    Ok(())
}
