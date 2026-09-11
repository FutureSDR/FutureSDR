#![cfg(not(target_arch = "wasm32"))]

use futuresdr::blocks::Fft;
use futuresdr::blocks::FftDirection;
use futuresdr::blocks::VectorSource;
use futuresdr::futures::future::Either;
use futuresdr::futures::future::select;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::mpsc_queue;
use futuresdr::runtime::buffer::slab;
use futuresdr::runtime::dev::prelude::*;
use std::time::Duration;
use wlan::Encoder;
use wlan::Mac;
use wlan::Mapper;
use wlan::Mcs;
use wlan::Prefix;

const PAYLOAD: &[u8] = b"shared queue WLAN regression";

#[derive(Block)]
struct WaveformSink {
    #[input]
    input: DefaultCpuReader<Complex32>,
    samples: Vec<Complex32>,
    expected: usize,
    result: mpsc::Sender<Vec<Complex32>>,
}

impl Kernel for WaveformSink {
    async fn work(
        &mut self,
        io: &mut WorkIo,
        _mo: &mut MessageOutputs,
        _meta: &BlockMeta,
    ) -> Result<()> {
        let input = self.input.slice();
        self.samples.extend_from_slice(input);
        let n = input.len();
        self.input.consume(n);
        if self.samples.len() == self.expected {
            self.result.send(std::mem::take(&mut self.samples)).await?;
            io.finished = true;
        }
        Ok(())
    }
}

fn waveform() -> Result<Vec<Complex32>> {
    let mut fg = Flowgraph::new();
    let mac = Mac::new([0x42; 6], [0x23; 6], [0xff; 6]);
    let encoder: Encoder = Encoder::new(Mcs::Qpsk_1_2);
    let mapper: Mapper = Mapper::new();
    let fft = Fft::with_options(
        64,
        FftDirection::Inverse,
        true,
        Some((1.0f32 / 52.0).sqrt() * 0.6),
    );
    let prefix: Prefix = Prefix::new(10000, 10000);
    let symbols = wlan::FrameParam::new(Mcs::Qpsk_1_2, PAYLOAD.len() + 28).n_symbols() + 1;
    let expected = 3 * (20_000 + 320 + symbols * 80);
    let (tx, rx) = mpsc::channel(1);
    let sink = WaveformSink {
        input: Default::default(),
        samples: Vec::new(),
        expected,
        result: tx,
    };
    connect!(fg, mac.tx | tx.encoder; encoder > mapper > fft > prefix > sink);
    let running = Runtime::new().start(fg)?;
    // Do not send Finished to Encoder: it terminates immediately, even with
    // frames still queued. Wait for the complete waveform before stopping TX.
    block_on(async {
        for _ in 0..3 {
            running
                .handle()
                .call(mac, "tx", Pmt::Blob(PAYLOAD.to_vec()))
                .await?;
        }
        Ok::<_, futuresdr::runtime::Error>(())
    })?;
    let samples = block_on(async {
        let received = std::pin::pin!(rx.recv());
        let timeout = std::pin::pin!(Timer::after(Duration::from_secs(10)));
        match select(received, timeout).await {
            Either::Left((samples, _)) => samples,
            Either::Right(_) => None,
        }
    });
    block_on(running.handle().stop())?;
    running.wait()?;
    Ok(samples.expect("TX did not produce the expected waveform"))
}

fn decode(dc_offset: bool) -> Result<()> {
    let samples = waveform()?;
    // A receiver retaining the removed million-sample startup drop cannot
    // decode this fixture. It also spans multiple default-sized queue pages.
    assert!(samples.len() < 1_000_000);
    let mut fg = Flowgraph::new();
    let source_domain = fg.local_domain()?;
    let source = fg.with_local_domain(source_domain, move |ctx| {
        if dc_offset {
            Ok(ctx
                .add(VectorSource::<Complex32, slab::Writer<Complex32>>::new(
                    samples,
                ))
                .id())
        } else {
            Ok(ctx
                .add(VectorSource::<Complex32, mpsc_queue::Writer<Complex32>>::new(samples))
                .id())
        }
    })?;
    let (tx, rx) = mpsc::channel(16);
    block_on(wlan_wasm::receiver::build_rx_flowgraph(
        &mut fg, source, "output", tx, dc_offset,
    ))?;
    let rt = Runtime::new();
    let running = rt.start(fg)?;
    let frame = block_on(async {
        let received = std::pin::pin!(rx.recv());
        let timeout = std::pin::pin!(Timer::after(Duration::from_secs(10)));
        match select(received, timeout).await {
            Either::Left((frame, _)) => frame,
            Either::Right(_) => None,
        }
    });
    block_on(running.handle().stop())?;
    running.wait()?;
    let frame = frame.expect("WLAN graph did not deliver a decoded frame");
    assert!(frame.windows(PAYLOAD.len()).any(|data| data == PAYLOAD));
    Ok(())
}

#[test]
fn receiver_decodes_with_dc_correction() -> Result<()> {
    decode(true)
}

#[test]
fn receiver_decodes_without_dc_correction() -> Result<()> {
    decode(false)
}
