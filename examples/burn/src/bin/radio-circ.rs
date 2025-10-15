#![recursion_limit = "512"]
use anyhow::Result;
use burn::prelude::*;
use burn::record::FullPrecisionSettings;
use burn::record::NamedMpkFileRecorder;
use burn::record::Recorder;
use clap::Parser;
use futuresdr::blocks::Apply;
use futuresdr::blocks::FirBuilder;
use futuresdr::blocks::XlatingFir;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::seify::Builder;
use futuresdr::futuredsp::firdes;
use futuresdr::macros::connect;
use futuresdr::num_complex::Complex32;
use futuresdr::prelude::*;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use whisper_burn::audio::max_waveform_samples;
use whisper_burn::audio::prep_audio;
use whisper_burn::model::Whisper;
use whisper_burn::model::WhisperConfig;
use whisper_burn::token::Gpt2Tokenizer;
use whisper_burn::token::Language;
use whisper_burn::transcribe::find_chunk_overlap;
use whisper_burn::transcribe::mels_to_text;

const PADDING: usize = 200;
const CHUNK_OVERLAP: usize = 16000 * 2;

// type B = burn::backend::Wgpu;
type B = burn::backend::Cuda;

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
    input: circular::Reader<f32>,
    language: Language,
    model: Whisper<B>,
    tokenizer: Gpt2Tokenizer,
    n_mels: usize,
    n_waveform_samples_per_window: usize,
    tokens: Vec<usize>,
    device: Device<B>,
}

impl WhisperBlock {
    fn new(device: &Device<B>) -> Self {
        let (tokenizer, _config, model) =
            load_model::<B>("/home/basti/src/whisper-burn/tiny", "tiny", device);

        let n_mels = model.encoder_mel_size();
        let n_waveform_samples_per_window =
            max_waveform_samples(model.encoder_ctx_size() - PADDING) / 2;
        println!("n_waveform_samples_per_window {n_waveform_samples_per_window}");
        let mut input: circular::Reader<f32> = Default::default();
        input.set_min_buffer_size_in_items(n_waveform_samples_per_window * 8);

        Self {
            input,
            language: Language::German,
            model,
            tokenizer,
            n_mels,
            n_waveform_samples_per_window,
            tokens: Vec::new(),
            device: device.clone(),
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
        let input = self.input.slice();
        let input_len = input.len();

        let n_samples_per_tensor = self.n_waveform_samples_per_window;
        let shift = n_samples_per_tensor.saturating_sub(CHUNK_OVERLAP).max(1);

        if input_len < n_samples_per_tensor {
            return Ok(());
        }
        let iter_len = (input_len - self.n_waveform_samples_per_window) / shift + 1;

        for i in 0..iter_len {
            let start = i * shift;
            let end = (start + n_samples_per_tensor).min(input_len);

            let slice = &input[start..end];
            println!("iter {i}  iter_len {iter_len}   slice len {}", slice.len());

            let waveform: Tensor<B, 1> =
                Tensor::from_data(TensorData::new(slice.to_vec(), [slice.len()]), &self.device);
            let mel = prep_audio(waveform.unsqueeze(), 16000.0, self.n_mels);

            let (new_text, new_tokens) =
                mels_to_text(&self.model, &self.tokenizer, self.language, mel, PADDING).unwrap();

            println!("new tokens: {new_tokens:?}");
            println!("new text: {new_text:?}");
            if let Some((prev_index, curr_index)) =
                find_chunk_overlap(&self.tokens[..], &new_tokens[..], 40, 3)
                && prev_index > 0
                && curr_index > 0
            {
                println!("prev index {prev_index}     curr_index {curr_index}");
                self.tokens.truncate(prev_index);
                self.tokens.extend(&new_tokens[curr_index..]);
            } else {
                let text = self.tokenizer.decode(&self.tokens[..], true).unwrap();
                println!("{text}");
                self.tokens = new_tokens;
            }
        }
        self.input.consume(iter_len * shift);
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
    #[clap(short, long, default_value_t = 0.128e6)]
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

    let cutoff = 4000.0 / args.intermediate_rate;
    let transition = 2000.0 / args.intermediate_rate;
    let audio_filter_taps = firdes::kaiser::lowpass::<f32>(cutoff, transition, 0.1);
    let resamp2 = FirBuilder::resampling_with_taps::<f32, f32, _>(1, 8, audio_filter_taps);
    let whisper = WhisperBlock::new(&device);
    let snk = AudioSink::new(args.audio_rate, 1);

    connect!(fg, src.outputs[0] > xlate > demod > resamp2 > whisper);
    connect!(fg, resamp2 > snk);

    Runtime::new().run(fg)?;
    Ok(())
}
