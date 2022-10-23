use futuresdr::anyhow::Result;
use futuresdr::async_io;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::signal_source::SignalSourceBuilder;
use futuresdr::blocks::zeromq::PubSinkBuilder;
use futuresdr::blocks::Oscillator;
//use futuresdr::blocks::Apply;
//use futuresdr::blocks::ApplyNM;
//use futuresdr::blocks::ConsoleSink;
//use futuresdr::blocks::Fft;
use futuresdr::blocks::Selector;
use futuresdr::blocks::SelectorDropPolicy;
use futuresdr::blocks::Source;
use futuresdr::num_complex::Complex32;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

use futuresdr::blocks::signal_source::FixedPointPhase;

fn main() -> Result<()> {
    let audio_rate: u32 = 48_000;

    let mut fg = Flowgraph::new();

    let center_freq: f32 = 440.0;
    let fwt0: f32 = -2.0 * std::f32::consts::PI * center_freq / (audio_rate as f32);

    // Oscillator to make sure selection is working
    let src0 = Oscillator::new(261.63, 0.3, audio_rate as f32);

    // Reference oscillator based on sinus, phase accumulator + modulo
    let src1 = Oscillator::new(center_freq, 1.0, audio_rate as f32);

    // Oscillator implementation based on complex polar and accumulator
    let mut osc = Complex32::new(1.0, 0.0);
    let shift = Complex32::from_polar(1.0, fwt0);
    let mut src2 = Source::new(move || {
        osc *= shift;
        osc.re
    });
    src2.set_instance_name(format!("polar based oscillator {}", center_freq));

    // Oscillator implementation based on index and multiplication, no f32 accumulator
    let mut local_oscillator_index: u32 = 1;
    let mut src3 = Source::new(move || {
        let lo_v = Complex32::new(0.0, (local_oscillator_index as f32) * fwt0).exp();
        local_oscillator_index = (local_oscillator_index + 1) % audio_rate;
        lo_v.re
    });
    src3.set_instance_name(format!(
        "index+multiplication based oscillator {}",
        center_freq
    ));

    // saw tooth
    let delta: u32 = (((std::u32::MAX) as f32) * center_freq / (audio_rate as f32)) as u32;
    let mut saw_oscillator_index: u32 = 0;
    let mut src4 = Source::new(move || {
        saw_oscillator_index = saw_oscillator_index.wrapping_add(delta);
        let saw_tooth: u32 = if saw_oscillator_index & (2u32.pow(31)) > 0 {
            std::u32::MAX - saw_oscillator_index
        } else {
            saw_oscillator_index
        };
        let saw_tooth = (saw_tooth as f32) / ((std::u32::MAX / 2u32) as f32);
        2.0 * (saw_tooth - 0.5)
    });
    src4.set_instance_name(format!("saw tooth {}", center_freq));

    // Oscillator implementation based on lookup table
    let delta_u16: u16 = (((std::u16::MAX) as f32) * center_freq / (audio_rate as f32)) as u16;
    let mut sin_lut = [0f32; (std::u16::MAX as usize)];
    // let mut osc_u16 = Complex32::new(1.0, 0.0);
    // let fwt0_u16: f32 = 2.0 * std::f32::consts::PI / (std::u16::MAX as f32);
    // let shift_u16 = Complex32::from_polar(1.0, fwt0_u16);
    // for i in 0..(std::u16::MAX as usize) {
    for (i, sin_lut_i) in sin_lut.iter_mut().enumerate().take(std::u16::MAX as usize) {
        // sin_lut[i]
        *sin_lut_i = f32::sin(2.0 * std::f32::consts::PI * (i as f32) / (std::u16::MAX as f32));
        // sin_lut[i] = osc_u16.re;
        // osc_u16 *= shift_u16;
    }
    let mut lut_oscillator_index: u16 = 0;
    let mut src5 = Source::new(move || {
        lut_oscillator_index = lut_oscillator_index.wrapping_add(delta_u16);
        sin_lut[lut_oscillator_index as usize]
    });
    src5.set_instance_name(format!("lookup table based oscillator {}", center_freq));

    // Oscillator implementation based on complex polar and accumulator
    let mut phase = FixedPointPhase::new(0.0);
    let phase_inc: FixedPointPhase = FixedPointPhase::new(fwt0);
    let mut src6 = Source::new(move || {
        let s = phase.sin();
        phase += phase_inc;
        s
    });
    src6.set_instance_name(format!("fixed point based {}", center_freq));

    let mut src7 = SignalSourceBuilder::<f32>::new()
        .for_sampling_rate(audio_rate as f32)
        .with_frequency(center_freq)
        .with_amplitude(1.0f32)
        .sine_wave();
    src7.set_instance_name(format!("Sine SignalSource {}", center_freq));

    // Select the implementation to hear from
    // Import to use same rate drop policy to be able to hear phase difference between implementations
    let selector = Selector::<f32, 8, 1>::new(SelectorDropPolicy::SameRate);
    // Store the `input_index` port ID for later use
    let input_index_port_id = selector
        .message_input_name_to_id("input_index")
        .expect("No input_index port found!");
    let snk = AudioSink::new(audio_rate, 1);

    let zmq_snk = PubSinkBuilder::<f32>::new()
        .address("tcp://127.0.0.1:50001")
        .min_item_per_send(100)
        .build();

    // Idea is to evaluate quality of the oscillator
    // So transform the real signal into complex and apply FFT
    // Only the first half part of the result would be significant
    // Look at which implementation is the more pure
    // const FFT_SIZE: usize = 2048;
    // let into_iq = Apply::new(|x: &f32| Complex32::new(*x, 0.0));
    // let fft = Fft::new(FFT_SIZE);
    // let pick = ApplyNM::<_, _, _, FFT_SIZE, 1>::new(move |v: &[Complex32], d: &mut [f32]| {
    //    let idx_max =
    //        v.iter().enumerate().fold(
    //            (0, 0.0),
    //            |max, (idx, &val)| if val.re > max.1 { (idx, val.re) } else { max },
    //        );
    //    // d[0] =  v[35];
    //    // d[0] = Complex32::new(idx_max.0 as f32, idx_max.1);
    //    let dbl_idx = (2 * idx_max.0).min(1024);
    //    //id[0] = Complex32::new(2.0 * (dbl_idx as f32), v[dbl_idx].re);
    //    d[0] = v[dbl_idx].re;
    // });
    // const AVG_SPAN: usize = 512;
    // let average = ApplyNM::<_, _, _, AVG_SPAN, 1>::new(move |v: &[f32], d: &mut [f32]| {
    //     d[0] = v.iter().sum::<f32>() / (AVG_SPAN as f32);
    // });
    // let console = ConsoleSink::<f32>::new("\n");

    let src0 = fg.add_block(src0);
    let src1 = fg.add_block(src1);
    let src2 = fg.add_block(src2);
    let src3 = fg.add_block(src3);
    let src4 = fg.add_block(src4);
    let src5 = fg.add_block(src5);
    let src6 = fg.add_block(src6);
    let src7 = fg.add_block(src7);
    let selector = fg.add_block(selector);
    let snk = fg.add_block(snk);
    let zmq_snk = fg.add_block(zmq_snk);

    fg.connect_stream(src0, "out", selector, "in0")?;
    fg.connect_stream(src1, "out", selector, "in1")?;
    fg.connect_stream(src2, "out", selector, "in2")?;
    fg.connect_stream(src3, "out", selector, "in3")?;
    fg.connect_stream(src4, "out", selector, "in4")?;
    fg.connect_stream(src5, "out", selector, "in5")?;
    fg.connect_stream(src6, "out", selector, "in6")?;
    fg.connect_stream(src7, "out", selector, "in7")?;
    fg.connect_stream(selector, "out0", snk, "in")?;
    fg.connect_stream(selector, "out0", zmq_snk, "in")?;

    // Start the flowgraph and save the handle
    let (_res, mut handle) = async_io::block_on(Runtime::new().start(fg));

    // Keep asking user for the selection
    loop {
        println!("Enter a new input index");
        // Get input from stdin and remove all whitespace (most importantly '\n' at the end)
        let mut input = String::new(); // Input buffer
        std::io::stdin()
            .read_line(&mut input)
            .expect("error: unable to read user input");
        input.retain(|c| !c.is_whitespace());

        if input.eq("quit") {
            break;
        }

        // If the user entered a valid number, set the new frequency by sending a message to the `FlowgraphHandle`
        if let Ok(new_index) = input.parse::<u32>() {
            println!("Setting source index to {}", input);
            async_io::block_on(handle.call(selector, input_index_port_id, Pmt::U32(new_index)))?;
        } else {
            println!("Input not parsable: {}", input);
        }
    }

    Ok(())
}
