use futuresdr::blocks::NullSink;
use futuresdr::blocks::VectorSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::DefaultLocalCpuReader;
use futuresdr::runtime::buffer::DefaultLocalCpuWriter;

fn main() -> Result<()> {
    let mut fg = Flowgraph::new();
    let source_domain = fg.local_domain()?;
    let sink_domain = fg.local_domain()?;
    let src = fg.with_local_domain(source_domain, |ctx| {
        Ok(ctx.add(VectorSource::<u8, DefaultLocalCpuWriter<u8>>::new(vec![1])))
    })?;
    let snk = fg.with_local_domain(sink_domain, |ctx| {
        Ok(ctx.add(NullSink::<u8, DefaultLocalCpuReader<u8>>::new()))
    })?;

    fg.stream(&src, |b| b.output(), &snk, |b| b.input())?;
    Ok(())
}
