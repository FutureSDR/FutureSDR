//! Shared 20 MHz source configuration after Seify device discovery.
use futuresdr::blocks::seify::AsyncBuilder;
use futuresdr::blocks::seify::AsyncSource;
use futuresdr::prelude::*;
use futuresdr::runtime::buffer::slab;
use futuresdr::seify::Driver;
use futuresdr::seify::DynAsyncDevice;
use std::time::Duration;

/// Sample clock required by the 20 MHz WLAN decoder.
pub const SAMPLE_RATE: f64 = 20_000_000.0;

/// Configure a radio without starting DMA; the source activates at graph init.
/// Empty arguments let Seify choose an accessible device and its backend.
pub async fn source(
    args: &str,
    frequency: f64,
    gain: f64,
) -> Result<(AsyncSource<DynAsyncDevice, slab::Writer<Complex32>>, Driver)> {
    let device = DynAsyncDevice::from_args(args.to_owned()).await?;
    let driver = device.driver();
    let mut builder = AsyncBuilder::from_dyn_device(device)
        .frequency(frequency)
        .sample_rate(SAMPLE_RATE)
        .gain(gain);
    if driver == Driver::Pluto {
        // Set the RX filter explicitly: a previous low-rate application may
        // have left it much narrower than this WLAN channel.
        builder = builder.bandwidth(SAMPLE_RATE).antenna("A_BALANCED");
    }
    let mut source = builder.build_source_with_buffer().await?;
    source.set_rate_check_interval(Duration::from_secs(5));
    Ok((source, driver))
}
