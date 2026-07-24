use clap::Parser;
use clap::ValueEnum;
use futuresdr::blocks::MessageSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use lora::Decoder;
use lora::Deinterleaver;
use lora::FftDemod;
use lora::FrameSync;
use lora::GrayMapping;
use lora::HammingDecoder;
use lora::HeaderDecoder;
use lora::build_lora_rx_dyn;
use lora::utils::Bandwidth;
use lora::utils::Channel;
use lora::utils::DeinterleavedSymbolHardDecoding;
use lora::utils::DemodulatedSymbolHardDecoding;
use lora::utils::HeaderMode;
use lora::utils::LdroMode;
use lora::utils::SpreadingFactor;
use lora::utils::SynchWord;
use perf::local_spsc;
use perf::local_spsc_tags;
use std::time;

const BW: Bandwidth = Bandwidth::BW125;
const SAMPLE_RATE: usize = 1_000_000;
const CHANNEL: Channel = Channel::Custom(869_525_000);
const BRANCHES: usize = 4;
const SF: SpreadingFactor = SpreadingFactor::SF7;

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
    #[clap(
        short,
        long,
        default_value = "lora-dumps/samples_sf7_pad16_snr30_16B_300s.cf32"
    )]
    file: String,
    /// Runtime config
    #[clap(long, value_enum, default_value_t = Config::Normal)]
    config: Config,
}

fn oversampling() -> usize {
    SAMPLE_RATE / Into::<usize>::into(BW)
}

fn load_cf32(path: &str) -> Result<Vec<Complex32>> {
    let bytes = std::fs::read(path)?;
    anyhow::ensure!(
        bytes.len().is_multiple_of(8),
        "invalid cf32 file size ({}), expected multiple of 8 bytes",
        bytes.len()
    );

    let mut samples = Vec::with_capacity(bytes.len() / 8);
    for chunk in bytes.chunks_exact(8) {
        let re = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let im = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
        samples.push(Complex32::new(re, im));
    }
    Ok(samples)
}

fn print_result(
    args: &Args,
    elapsed: time::Duration,
    terminated: &TerminatedFlowgraph,
    sinks: &[BlockRef<MessageSink>],
) -> Result<()> {
    let mut counts = Vec::with_capacity(sinks.len());
    for sink in sinks {
        counts.push(terminated.with(sink, |sink| sink.received())?);
    }

    println!(
        "{},{},{},{},{},{},{},{}",
        args.run,
        args.file,
        match args.config {
            Config::Normal => "normal",
            Config::Opti => "opti",
        },
        elapsed.as_secs_f64(),
        counts[0],
        counts[1],
        counts[2],
        counts[3]
    );

    Ok(())
}

fn normal(args: Args) -> Result<()> {
    let mut samples = Some(load_cf32(&args.file)?);
    let mut fg = Flowgraph::new();
    let mut sinks = Vec::with_capacity(BRANCHES);

    for branch in 0..BRANCHES {
        let src_samples = if branch + 1 == BRANCHES {
            samples.take().expect("samples already moved")
        } else {
            samples.as_ref().expect("samples missing").clone()
        };
        let src = fg.add(VectorSource::<Complex32>::new(src_samples))?;
        let (frame_sync, decoder) = build_lora_rx_dyn(
            &mut fg,
            CHANNEL,
            BW,
            SF,
            HeaderMode::Explicit,
            LdroMode::AUTO,
            Some(&[SynchWord::Public]),
            oversampling(),
            None,
            None,
            false,
            None,
            false,
        )?;
        let sink = fg.add(MessageSink::new())?;

        fg.stream_dyn(src, "output", frame_sync, "input")?;
        fg.message(decoder, "crc_check", sink, "in")?;
        sinks.push(sink);
    }

    let runtime = Runtime::new();
    let now = time::Instant::now();
    let terminated = runtime.run(fg)?;
    let elapsed = now.elapsed();

    print_result(&args, elapsed, &terminated, &sinks)
}

fn pinned_local_domains(fg: &mut Flowgraph, n: usize) -> Result<Vec<LocalDomain>> {
    let cores = core_affinity::get_core_ids().unwrap_or_default();
    if cores.len() < n {
        return Err(anyhow::anyhow!(
            "optimized LoRa benchmark needs {n} available CPUs, got {}",
            cores.len()
        ));
    }

    cores
        .into_iter()
        .take(n)
        .map(|core| fg.local_domain_pinned(core.id).map_err(Into::into))
        .collect()
}

fn opti(args: Args) -> Result<()> {
    type SourceComplexReader = local_spsc::Reader<Complex32>;
    type SourceComplexWriter = local_spsc::Writer<Complex32>;
    type LocalSpscTagsComplexReader = local_spsc_tags::Reader<Complex32>;
    type LocalSpscTagsComplexWriter = local_spsc_tags::Writer<Complex32>;
    type LocalSpscTagsU16Reader = local_spsc_tags::Reader<DemodulatedSymbolHardDecoding>;
    type LocalSpscTagsU16Writer = local_spsc_tags::Writer<DemodulatedSymbolHardDecoding>;
    type LocalSpscTagsU8Reader = local_spsc_tags::Reader<u8>;
    type LocalSpscTagsU8Writer = local_spsc_tags::Writer<u8>;

    let mut samples = Some(load_cf32(&args.file)?);
    let mut fg = Flowgraph::new();
    let domains = pinned_local_domains(&mut fg, BRANCHES)?;
    let mut sinks = Vec::with_capacity(BRANCHES);

    for (branch, local) in domains.into_iter().enumerate() {
        let src_samples = if branch + 1 == BRANCHES {
            samples.take().expect("samples already moved")
        } else {
            samples.as_ref().expect("samples missing").clone()
        };
        let ldro_enabled = LdroMode::AUTO.resolve_if_auto(SF, BW).enabled();
        let (
            src,
            frame_sync,
            fft_demod,
            gray_mapping,
            deinterleaver,
            hamming_dec,
            header_decoder,
            decoder,
            sink,
        ) = fg.with_local_domain(local, move |ctx| {
            let src = ctx.add(VectorSource::<Complex32, SourceComplexWriter>::new(
                src_samples,
            ));
            let frame_sync = ctx.add(
                FrameSync::<SourceComplexReader, LocalSpscTagsComplexWriter>::new(
                    CHANNEL,
                    BW,
                    SF,
                    false,
                    &[SynchWord::Public],
                    oversampling(),
                    None,
                    None,
                    false,
                    None,
                ),
            );
            let fft_demod = ctx.add(FftDemod::<
                DemodulatedSymbolHardDecoding,
                lora::fft_demod::State<DemodulatedSymbolHardDecoding>,
                LocalSpscTagsComplexReader,
                LocalSpscTagsU16Writer,
            >::new(SF, ldro_enabled));
            let gray_mapping = ctx.add(GrayMapping::<
                DemodulatedSymbolHardDecoding,
                LocalSpscTagsU16Reader,
                LocalSpscTagsU16Writer,
            >::new());
            let deinterleaver = ctx.add(Deinterleaver::<
                DemodulatedSymbolHardDecoding,
                DeinterleavedSymbolHardDecoding,
                LocalSpscTagsU16Reader,
                LocalSpscTagsU8Writer,
            >::new(ldro_enabled, SF));
            let hamming_dec = ctx.add(HammingDecoder::<
                DeinterleavedSymbolHardDecoding,
                LocalSpscTagsU8Reader,
                LocalSpscTagsU8Writer,
            >::new());
            let header_decoder = ctx.add(HeaderDecoder::<LocalSpscTagsU8Reader>::new(
                HeaderMode::Explicit,
                ldro_enabled,
            ));
            let decoder = ctx.add(Decoder::new());
            let sink = ctx.add(MessageSink::new());

            Ok((
                src,
                frame_sync,
                fft_demod,
                gray_mapping,
                deinterleaver,
                hamming_dec,
                header_decoder,
                decoder,
                sink,
            ))
        })?;

        fg.stream_dyn(src, "output", frame_sync, "input")?;
        fg.stream_dyn(frame_sync, "output", fft_demod, "input")?;
        fg.stream_dyn(fft_demod, "output", gray_mapping, "input")?;
        fg.stream_dyn(gray_mapping, "output", deinterleaver, "input")?;
        fg.stream_dyn(deinterleaver, "output", hamming_dec, "input")?;
        fg.stream_dyn(hamming_dec, "output", header_decoder, "input")?;
        fg.message(header_decoder, "frame_info", frame_sync, "frame_info")?;
        fg.message(header_decoder, "out", decoder, "in")?;
        fg.message(decoder, "crc_check", sink, "in")?;
        sinks.push(sink);
    }

    let runtime = Runtime::new();
    let now = time::Instant::now();
    let terminated = runtime.run(fg)?;
    let elapsed = now.elapsed();

    print_result(&args, elapsed, &terminated, &sinks)
}

fn main() -> Result<()> {
    let args = Args::parse();
    futuresdr::runtime::config::set("ctrlport_enable", false);
    futuresdr::runtime::config::set("log_level", "OFF");

    match args.config {
        Config::Normal => normal(args)?,
        Config::Opti => opti(args)?,
    }

    Ok(())
}
