use anyhow::Result;
use clap::Parser;
use futuresdr::async_io;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::Selector;
use futuresdr::blocks::SelectorDropPolicy as DropPolicy;
use futuresdr::blocks::SignalSource;
use futuresdr::blocks::SignalSourceBuilder;
use futuresdr::prelude::*;

#[derive(Parser, Debug)]
struct Args {
    // Drop policy to apply on the selector.
    #[clap(short, long, default_value = "same")]
    drop_policy: DropPolicy,
}

fn main() -> Result<()> {
    let args = Args::parse();
    println!("Configuration {args:?}");

    let mut fg = Flowgraph::new();

    let src0 = SignalSourceBuilder::<f32, _, circular::Writer<f32>>::sinf32(440.0, 48000.0)
        .amplitude(0.3)
        .build();
    let src1 = SignalSourceBuilder::sinf32(261.63, 48000.0)
        .amplitude(0.3)
        .build();
    let selector = Selector::<f32, 2, 1>::new(args.drop_policy);
    let snk = AudioSink::new(48_000, 1);

    connect!(fg, src0 > inputs[0].selector);
    connect!(fg, src1 > inputs[1].selector);
    connect!(fg, selector.outputs[0] > snk);
    let selector = selector.get().id;

    // Start the flowgraph and save the handle
    let rt = Runtime::new();
    let (_res, mut handle) = rt.start_sync(fg);

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
            println!("Setting source index to {input}");
            async_io::block_on(handle.call(selector, "input_index", Pmt::U32(new_index)))?;
        } else {
            println!("Input not parsable: {input}");
        }
    }

    Ok(())
}
