use anyhow::Result;
use futuresdr::blocks::FileSink;
use futuresdr::blocks::FileSource;
use futuresdr::runtime::Flowgraph;
use futuresdr::runtime::Runtime;
use std::time;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() != 3 {
        println!("usage: file <input file> <output file>");
        return Ok(());
    }

    let mut fg = Flowgraph::new();

    let src = fg.add_block(FileSource::<u8>::new(&args[1], false));
    let snk = fg.add_block(FileSink::<u8>::new(&args[2]));

    fg.connect_stream(src.get().output(), snk.get().input());

    let now = time::Instant::now();
    Runtime::new().run(fg)?;
    let elapsed = now.elapsed();

    println!("flowgraph took {elapsed:?}");

    Ok(())
}
