#![cfg(not(target_arch = "wasm32"))]

use futuresdr::prelude::*;
use futuresdr::runtime::buffer::slab;
use futuresdr::runtime::dev::prelude::*;
use futuresdr::seify::Driver;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use wlan_wasm::radio::SAMPLE_RATE;

#[derive(Default)]
struct Statistics {
    samples: usize,
    nonzero: bool,
}

#[derive(Block)]
struct CaptureCount {
    #[input]
    input: slab::Reader<Complex32>,
    #[output]
    output: slab::Writer<Complex32>,
    stats: Arc<Mutex<Statistics>>,
}

impl Kernel for CaptureCount {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let finished = self.input.finished();
        let input = self.input.slice();
        let output = self.output.slice();
        let n = input.len().min(output.len());
        let done = finished && n == input.len();
        let input = &input[..n];
        assert!(input.iter().all(|s| s.re.is_finite() && s.im.is_finite()));
        output[..n].copy_from_slice(input);
        {
            let mut stats = self.stats.lock().unwrap();
            stats.samples += n;
            stats.nonzero |= input.iter().any(|s| s.norm_sqr() > 0.0);
        }
        self.input.consume(n);
        self.output.produce(n);
        if done {
            io.finished = true;
        }
        Ok(())
    }
}

/// This checks real source delivery and reconfiguration through the WLAN graph.
/// Received frames depend on local RF traffic; this is not a lossless-rate test.
#[test]
#[ignore = "requires a Pluto; configures channel 11 at 20 MS/s for five seconds"]
fn pluto_source_runs_wlan_graph_and_reconfigures() -> Result<()> {
    futuresdr::runtime::config::set("buffer_size", (512 * 1024) as i64);
    let mut fg = Flowgraph::new();
    let source_domain = fg.local_domain()?;
    let source = block_on(fg.with_local_domain_async(
        source_domain,
        async move |ctx: &LocalDomainContext<'_>| {
            let (source, driver, software_dc) =
                wlan_wasm::radio::source("driver=pluto", 2_462_000_000.0, 50.0).await?;
            assert_eq!(driver, Driver::Pluto);
            assert!(!software_dc);
            Ok(ctx.add(source).id())
        },
    ))?;
    let stats = Arc::new(Mutex::new(Statistics::default()));
    let stats_for_sink = stats.clone();
    let monitor_domain = fg.local_domain()?;
    let monitor = fg.with_local_domain(monitor_domain, move |ctx| {
        Ok(ctx
            .add(CaptureCount {
                input: Default::default(),
                output: Default::default(),
                stats: stats_for_sink,
            })
            .id())
    })?;
    fg.stream_dyn(source, "outputs[0]", monitor, "input")?;
    let (frames_tx, frames_rx) = mpsc::channel(100);
    block_on(wlan_wasm::receiver::build_rx_flowgraph(
        &mut fg, monitor, "output", frames_tx, false,
    ))?;
    let running = Runtime::new().start(fg)?;
    let start = Instant::now();
    let result = block_on(async {
        let source = running.handle().block(source);
        let Pmt::F64(frequency) = source.call("freq", Pmt::Null).await? else {
            panic!("expected frequency readback");
        };
        // AD936x PLL readback can differ by a few Hz from the request.
        assert!((frequency - 2_462_000_000.0).abs() < 10.0);
        let Pmt::F64(rate) = source.call("sample_rate", Pmt::Null).await? else {
            panic!("expected sample-rate readback");
        };
        assert!((rate - SAMPLE_RATE).abs() < 5.0);
        Timer::after(Duration::from_secs(3)).await;
        let before = stats.lock().unwrap().samples;
        assert!(before > 0);
        assert_eq!(source.call("gain", Pmt::F64(45.0)).await?, Pmt::Ok);
        assert_eq!(
            source.call("freq", Pmt::F64(2_437_000_000.0)).await?,
            Pmt::Ok
        );
        let Pmt::F64(frequency) = source.call("freq", Pmt::Null).await? else {
            panic!("expected frequency readback after retune");
        };
        assert!((frequency - 2_437_000_000.0).abs() < 10.0);
        assert_eq!(source.call("gain", Pmt::Null).await?, Pmt::F64(45.0));
        Timer::after(Duration::from_secs(2)).await;
        assert!(stats.lock().unwrap().samples > before);
        Ok::<_, futuresdr::runtime::Error>(())
    });
    block_on(running.handle().stop())?;
    running.wait()?;
    result?;
    let stats = stats.lock().unwrap();
    assert!(stats.samples >= 1_000_000 && stats.nonzero);
    let mut frames = 0;
    while frames_rx.try_recv().is_ok() {
        frames += 1;
    }
    eprintln!(
        "Pluto WLAN graph: {} samples, {:.2} MS/s delivered, {} decoded frames; gain/retune and shutdown passed",
        stats.samples,
        stats.samples as f64 / start.elapsed().as_secs_f64() / 1e6,
        frames
    );
    Ok(())
}
