use anyhow::Result;
use futuresdr::blocks::Head;
use futuresdr::blocks::NullSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::DefaultCpuReader;
use futuresdr::runtime::buffer::DefaultCpuWriter;
use futuresdr::runtime::buffer::DefaultLocalCpuReader;
use futuresdr::runtime::buffer::DefaultLocalCpuWriter;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();
    let local = fg.local_domain()?;

    let head = fg.with_local_domain(local, |ctx| {
        let src = ctx.add(VectorSource::<u8, DefaultLocalCpuWriter<u8>>::new(vec![
            1, 2, 3, 4,
        ]));
        let head = ctx.add(Head::<u8, DefaultLocalCpuReader<u8>, DefaultCpuWriter<u8>>::new(3));
        connect!(ctx, src ~> head);
        Ok(head)
    })?;
    let snk = fg.add(NullSink::<u8, DefaultCpuReader<u8>>::new())?;

    fg.stream(&head, |b| b.output(), &snk, |b| b.input())?;

    let fg = Runtime::new().run(fg)?;
    let snk = fg.block(&snk)?;

    println!("received {} items", snk.n_received());

    Ok(())
}
