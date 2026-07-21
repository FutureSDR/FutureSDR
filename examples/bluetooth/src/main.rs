use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use futuresdr::prelude::*;
use futuresdr::runtime::Error as RuntimeError;

use crate::cli::Args;

mod ble_ad;
mod ble_burst_catcher;
mod ble_burst_decoder;
mod ble_burst_stats;
mod ble_channel_selection;
mod ble_connection;
mod ble_connection_follower;
mod ble_connection_report;
mod ble_control;
mod ble_data;
mod ble_detector;
mod ble_iq_burst;
mod ble_phy;
mod ble_protocol;
mod ble_sim;
mod ble_slicer;
mod ble_sync;
mod ble_timing_recovery;
mod ble_wireshark;
mod channelizer;
mod cli;
mod diagnostics;
mod flowgraph;
mod gmsk_demod;

fn main() -> Result<()> {
    futuresdr::runtime::init();
    let args = Args::parse();

    println!("Starting Bluetooth GMSK sniffer example");
    let mut fg = Flowgraph::new();
    let selected_channels = cli::selected_channels(&args)?;
    let mut sample_rate = args.sample_rate;

    cli::validate(&args)?;

    if args.is_multi_channel() && (sample_rate - 2.0e6).abs() < f64::EPSILON {
        let center_frequency = channelizer::center_frequency_for_channels(&selected_channels)?;
        sample_rate =
            channelizer::auto_multi_channel_sample_rate(&selected_channels, center_frequency)?;
        println!(
            "Auto-selected {:.3} MS/s for multi-channel capture. Override with --sample-rate if needed.",
            sample_rate / 1.0e6
        );
    }

    if args.is_multi_channel() {
        if args.simulate {
            bail!("--channels cannot be combined with --simulate yet");
        }
        if args.iq_file.is_some() {
            bail!("--channels cannot be combined with --iq-file yet");
        }
        flowgraph::build_multi_channel_flowgraph(&mut fg, &args, sample_rate, &selected_channels)?;
    } else {
        flowgraph::build_single_channel_flowgraph(&mut fg, &args, sample_rate)?;
    }

    if args.simulate || args.iq_file.is_some() {
        if args.duration.is_some() {
            println!("Finite input mode: ignoring --duration and running until input EOF.");
        }
        println!("Runtime started.");
        Runtime::new().run(fg)?;
        println!("Runtime stopped.");
        return Ok(());
    }

    let rt = Runtime::new();
    let running = rt.start(fg)?;
    println!("Runtime started.");

    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = shutdown_tx.send(());
    })?;

    if let Some(duration) = args.duration {
        let _ = shutdown_rx.recv_timeout(Duration::from_secs_f64(duration));
    } else {
        println!("Press Ctrl+C to stop.");
        shutdown_rx.recv()?;
    }

    Runtime::block_on(async move {
        match running.stop().await {
            Ok(()) | Err(RuntimeError::FlowgraphTerminated) => {}
            Err(err) => return Err(err.into()),
        }
        match running.wait_async().await {
            Ok(_) | Err(RuntimeError::FlowgraphTerminated) => {}
            Err(err) => return Err(err.into()),
        }
        Ok::<(), anyhow::Error>(())
    })?;

    println!("Runtime stopped.");
    Ok(())
}
