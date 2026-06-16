use anyhow::Result;
use yagi::filter::fir_design_gmskrx;

/// Create receive filter taps for a GMSK signal.
///
/// `samples_per_symbol` is the number of ADC samples per BLE/GFSK symbol.
pub fn gmsk_rx_taps(samples_per_symbol: usize) -> Result<Vec<f32>> {
    // BLE uses a Gaussian BT of 0.5 and a filter span of about 3 symbols.
    let beta = 0.5;
    let symbol_delay = 3;
    let dt = 0.0;

    let taps = fir_design_gmskrx(samples_per_symbol, symbol_delay, beta, dt)?;
    Ok(taps)
}
