use clap::Parser;
use futuresdr::anyhow::Result;
use futuresdr::async_io;
use futuresdr::blocks::audio::AudioSink;
use futuresdr::blocks::Selector;
use futuresdr::blocks::SelectorDropPolicy as DropPolicy;
use futuresdr::blocks::SignalSourceBuilder;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Pmt;
use futuresdr::runtime::Runtime;

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

    let src0 = SignalSourceBuilder::<f32>::sin(440.0, 48000.0)
        .amplitude(0.3)
        .build();
    let src1 = SignalSourceBuilder::<f32>::sin(261.63, 48000.0)
        .amplitude(0.3)
        .build();
    let selector = Selector::<f32, 2, 1>::new(args.drop_policy);
    // Store the `input_index` port ID for later use
    let input_index_port_id = selector
        .message_input_name_to_id("input_index")
        .expect("No input_index port found!");
    let snk = AudioSink::new(48_000, 1);

    let src0 = fg.add_block(src0);
    let src1 = fg.add_block(src1);
    let selector = fg.add_block(selector);
    let snk = fg.add_block(snk);

    fg.connect_stream(src0, "out", selector, "in0")?;
    fg.connect_stream(src1, "out", selector, "in1")?;
    fg.connect_stream(selector, "out0", snk, "in")?;

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
            println!("Setting source index to {input}");
            async_io::block_on(handle.call(selector, input_index_port_id, Pmt::U32(new_index)))?;
        } else {
            println!("Input not parsable: {input}");
        }
    }

    Ok(())
}
