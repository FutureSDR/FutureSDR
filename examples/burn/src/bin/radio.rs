#![recursion_limit = "512"]
use anyhow::Result;
// use burn::backend::WebGpu;
// use burn::backend::wgpu::WgpuRuntime;
// use burn::backend::wgpu::init_setup;
use burn::prelude::*;
use burn::record::FullPrecisionSettings;
use burn::record::NamedMpkFileRecorder;
use burn::record::Recorder;
// use burn::tensor::DType;
// use burn::tensor::TensorPrimitive;
// use burn_cubecl::CubeBackend;
// use burn_cubecl::ops::numeric::empty_device;
// use burn_cubecl::tensor::CubeTensor;
// use burn_fusion::Fusion;
// use burn_fusion::client::FusionClient;
// use burn_fusion::stream::StreamId;
use clap::Parser;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::Split;
use futuresdr::blocks::XlatingFir;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::seify::Builder;
use futuresdr::futuredsp::firdes;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::prelude::*;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
// use whisper_burn::audio::prep_audio;
use whisper_burn::model::Whisper;
use whisper_burn::model::WhisperConfig;
use whisper_burn::token::Gpt2Tokenizer;
// use whisper_burn::token::Language;
// use whisper::transcribe::find_chunk_overlap;
// use whisper_burn::transcribe::mels_to_text;

// const PADDING: usize = 200;

type B = burn::backend::Wgpu;
// type B = burn::backend::Cuda;

fn load_model<B: Backend>(
    model_path: &str,
    model_name: &str,
    device: &B::Device,
) -> (Gpt2Tokenizer, WhisperConfig, Whisper<B>) {
    let bpe = match Gpt2Tokenizer::new(model_path) {
        Ok(bpe) => bpe,
        Err(e) => {
            eprintln!("Failed to load tokenizer: {e}");
            std::process::exit(1);
        }
    };

    println!("name {model_name}");
    let whisper_config = match WhisperConfig::load(format!("{model_path}/{model_name}.cfg")) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to load whisper config: {e}");
            std::process::exit(1);
        }
    };

    println!("Loading model...");
    let whisper: Whisper<B> = {
        match NamedMpkFileRecorder::<FullPrecisionSettings>::new()
            .load(format!("{model_path}/{model_name}").into(), device)
            .map(|record| whisper_config.init(device).load_record(record))
        {
            Ok(whisper_model) => whisper_model,
            Err(e) => {
                eprintln!("Failed to load whisper model file: {e}");
                std::process::exit(1);
            }
        }
    };

    let whisper = whisper.to_device(device);
    (bpe, whisper_config, whisper)
}

#[derive(Block)]
struct WhisperBlock {
    #[input]
    input: burn_buffer::Reader<B, Float>,
    // language: Language,
    // model: Whisper<B>,
    // tokenizer: Gpt2Tokenizer,
    // n_mels: usize,
    // tokens: Vec<usize>,
}

impl WhisperBlock {
    fn new(device: &Device<B>) -> Self {
        let (_tokenizer, _config, _model) =
            load_model::<B>("/home/basti/src/whisper-burn/tiny", "tiny", device);

        // let n_mels = model.encoder_mel_size();
        Self {
            input: Default::default(),
            // language: Language::German,
            // model,
            // tokenizer,
            // n_mels,
            // tokens: Vec::new(),
        }
    }
}

impl Kernel for WhisperBlock {
    async fn work(
        &mut self,
        _io: &mut WorkIo,
        _m: &mut MessageOutputs,
        _b: &mut BlockMeta,
    ) -> Result<()> {
        if let Some(_b) = self.input.get_full_buffer() {
            // let jit_tensor = b.into_tensor().into_primitive();
            // let jit_tensor = jit_tensor.tensor();
            // // let ir = jit_tensor.into_ir();
            //
            // let client = jit_tensor.client;
            // let float_tensor = client.resolve_tensor_float::<B>(jit_tensor);

            // let tensor = b.into_tensor();
            // let prim = tensor.into_primitive().tensor();
            // let client = prim.client;
            // let resource_handle = client.get_resource(prim.handle.binding());
            // let buffer = resource_handle.resource().buffer();

            // // Get the 'resource' from the client
            // let resource = jit_tensor.client.get_resource(jit_tensor.handle.clone().binding());
            // // Which has the wgpu buffer.
            // let buffer = resource.buffer;
            // // But do note it only uses a part of the buffer, see offset + size.
            // let offset = resource.offset;
            // // Nb: For the buffer to be in a consistent state first flush all pending work if needed.
            // jit_tensor.client.sync(burn::tensor::backend::SyncType::Flush);

            // let waveform = b.into_tensor();
            // let mel = prep_audio(waveform.unsqueeze(), 16000.0, self.n_mels);
            //
            // let (new_text, _new_tokens) =
            //     mels_to_text(&self.model, &self.tokenizer, self.language, mel, PADDING).unwrap();

            // if let Some((prev_index, curr_index)) =
            //     find_chunk_overlap(&self.tokens[..], &new_tokens[..], 40, 3)
            // {
            //     self.tokens.truncate(prev_index);
            //     self.tokens.extend(&new_tokens[curr_index..]);
            // } else {
            //     self.tokens.extend(new_tokens);
            // }
            //
            // let text = self.tokenizer.decode(&self.tokens[..], true).unwrap();
            // println!("\nText: {new_text}");
            self.input.notify_consumed_buffer();
        }
        Ok(())
    }
}

#[derive(Parser, Debug)]
struct Args {
    /// Gain to apply to the seify source
    #[clap(short, long, default_value_t = 45.0)]
    gain: f64,
    /// Center frequency
    #[clap(short, long, default_value_t = 105.3e6)]
    frequency: f64,
    /// Frequency Offset
    #[clap(short, long, default_value_t = 0.3e6)]
    frequency_offset: f64,
    /// Sample rate
    #[clap(short, long, default_value_t = 1.28e6)]
    sample_rate: f64,
    /// Intermediate rate
    #[clap(short, long, default_value_t = 0.04e6)]
    intermediate_rate: f64,
    /// Seify args
    #[clap(short, long, default_value = "")]
    args: String,
    /// Audio Rate
    #[clap(long, default_value_t = 16000)]
    audio_rate: u32,
}

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let args = Args::parse();
    println!("Configuration {args:?}");
    let device = Default::default();
    // let setup = init_setup::<burn_wgpu::graphics::WebGpu>(&device, Default::default());
    // let queue = setup.queue;

    let mut fg = Flowgraph::new();
    let src = Builder::new(args.args)?
        .frequency(args.frequency - args.frequency_offset)
        .sample_rate(args.sample_rate)
        .gain(args.gain)
        .build_source()?;

    let xlate: XlatingFir =
        XlatingFir::new(10, args.frequency_offset as f32, args.sample_rate as f32);

    let mut last = Complex32::new(1.0, 0.0);
    let demod = Apply::<_, _, _>::new(move |v: &Complex32| -> f32 {
        let arg = (v * last.conj()).arg();
        last = *v;
        arg / 8.0
    });

    let cutoff = 6000.0 / args.intermediate_rate;
    let transition = 3000.0 / args.intermediate_rate;
    let audio_filter_taps = firdes::kaiser::lowpass::<f32>(cutoff, transition, 0.1);
    let resamp2 = FirBuilder::resampling_with_taps::<f32, f32, _>(1, 8, audio_filter_taps);
    let mut split: Split<
        _,
        _,
        _,
        _,
        circular::Reader<f32>,
        burn_buffer::Writer<B, Float>,
        circular::Writer<f32>,
    > = Split::new(|i: &f32| (*i, *i));
    split.output0().set_device(&device);
    let whisper = WhisperBlock::new(&device);
    let snk = AudioSink::new(args.audio_rate, 1);

    // let n_ctx_max_encoder = whisper.model.encoder_ctx_size();
    // let n_waveform_samples_per_window =
    //     whisper::audio::max_waveform_samples(n_ctx_max_encoder - PADDING);
    let n_waveform_samples_per_window = 1024 * 256;
    println!("waveform samples {n_waveform_samples_per_window}");
    split
        .output0()
        .inject_buffers_with_items(4, n_waveform_samples_per_window);

    connect!(fg, src.outputs[0] > xlate > demod > resamp2 > input.split.output0 > whisper);
    connect!(fg, split.output0 < whisper);
    connect!(fg, split.output1 > snk);

    Runtime::new().run(fg)?;
    Ok(())
}
