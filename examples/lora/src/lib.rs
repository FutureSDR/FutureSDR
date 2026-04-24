#![allow(clippy::precedence)]

use crate::utils::Bandwidth;
use crate::utils::Channel;
use crate::utils::CodeRate;
use crate::utils::DeinterleavedSymbolHardDecoding;
use crate::utils::DemodulatedSymbolHardDecoding;
use crate::utils::HeaderMode;
use crate::utils::LdroMode;
use crate::utils::SpreadingFactor;
use crate::utils::SynchWord;
pub use decoder::Decoder;
pub use deinterleaver::Deinterleaver;
pub use encoder::Encoder;
pub use fft_demod::FftDemod;
pub use frame_sync::FrameSync;
use futuresdr::prelude::BlockRef;
use futuresdr::prelude::Flowgraph;
use futuresdr::prelude::Result;
use futuresdr::prelude::connect;
pub use gray_mapping::GrayMapping;
pub use hamming_dec::HammingDecoder;
pub use header_decoder::Frame;
pub use header_decoder::HeaderDecoder;
pub use modulator::Modulator;
pub use packet_forwarder_client::PacketForwarderClient;
use std::time::SystemTime;
pub use stream_adder::StreamAdder;
pub use transmitter::Transmitter;

pub mod decoder;
pub mod default_values;
pub mod deinterleaver;
pub mod encoder;
pub mod fft_demod;
pub mod frame_sync;
pub mod gray_mapping;
pub mod hamming_dec;
pub mod header_decoder;
pub mod meshtastic;
pub mod modulator;
pub mod packet_forwarder_client;
pub mod stream_adder;
pub mod transmitter;
pub mod utils;

#[allow(clippy::too_many_arguments)]
pub fn build_lora_tx(
    fg: &mut Flowgraph,
    bw: Bandwidth,
    sf: SpreadingFactor,
    code_rate: CodeRate,
    has_crc: bool,
    ldro: LdroMode,
    header_mode: HeaderMode,
    os_factor: usize,
    sync_word: SynchWord,
    preamble_len: Option<usize>,
    pad: usize,
) -> Result<BlockRef<Transmitter>> {
    let ldro_enabled = ldro.resolve_if_auto(sf, bw).enabled();
    let transmitter = fg.add(Transmitter::new(
        code_rate,
        has_crc,
        sf,
        ldro_enabled,
        header_mode,
        os_factor,
        sync_word,
        preamble_len.unwrap_or(default_values::preamble_len(sf)),
        pad,
    )?);
    Ok(transmitter)
}

#[allow(clippy::too_many_arguments)]
pub fn build_lora_rx_dyn(
    fg: &mut Flowgraph,
    chan: Channel,
    bw: Bandwidth,
    sf: SpreadingFactor,
    header_mode: HeaderMode,
    ldro: LdroMode,
    initial_sync_words: Option<&[SynchWord]>,
    os_factor: usize,
    preamble_len: Option<usize>,
    sync_word_caching_policy: Option<&str>,
    collect_receive_statistics: bool,
    startup_timestamp: Option<SystemTime>,
    soft_decoding: bool,
) -> Result<(BlockRef<FrameSync>, BlockRef<Decoder>)> {
    if soft_decoding {
        let (frame_sync_ref, decoder_ref) = build_lora_rx_soft_decoding(
            fg,
            chan,
            bw,
            sf,
            header_mode,
            ldro,
            initial_sync_words,
            os_factor,
            preamble_len,
            sync_word_caching_policy,
            collect_receive_statistics,
            startup_timestamp,
        )?;
        Ok((frame_sync_ref, decoder_ref))
    } else {
        let (frame_sync_ref, decoder_ref) = build_lora_rx_hard_decoding(
            fg,
            chan,
            bw,
            sf,
            header_mode,
            ldro,
            initial_sync_words,
            os_factor,
            preamble_len,
            sync_word_caching_policy,
            collect_receive_statistics,
            startup_timestamp,
        )?;
        Ok((frame_sync_ref, decoder_ref))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_lora_rx_soft_decoding(
    mut fg: &mut Flowgraph,
    chan: Channel,
    bw: Bandwidth,
    sf: SpreadingFactor,
    header_mode: HeaderMode,
    ldro: LdroMode,
    initial_sync_words: Option<&[SynchWord]>,
    os_factor: usize,
    preamble_len: Option<usize>,
    sync_word_caching_policy: Option<&str>,
    collect_receive_statistics: bool,
    startup_timestamp: Option<SystemTime>,
) -> Result<(BlockRef<FrameSync>, BlockRef<Decoder>)> {
    let ldro_enabled = ldro.resolve_if_auto(sf, bw).enabled();
    let frame_sync: FrameSync = FrameSync::new(
        chan,
        bw,
        sf,
        !matches!(header_mode, HeaderMode::Explicit),
        initial_sync_words.unwrap_or_default(),
        os_factor,
        preamble_len,
        sync_word_caching_policy,
        collect_receive_statistics,
        startup_timestamp,
    );
    let fft_demod: FftDemod = FftDemod::new(sf, ldro_enabled);
    let gray_mapping: GrayMapping = GrayMapping::new();
    let deinterleaver: Deinterleaver = Deinterleaver::new(ldro_enabled, sf);
    let hamming_dec: HammingDecoder = HammingDecoder::new();
    let header_decoder: HeaderDecoder = HeaderDecoder::new(header_mode, ldro_enabled);
    let decoder: Decoder = Decoder::new();
    connect!(fg,
        frame_sync > fft_demod > gray_mapping > deinterleaver > hamming_dec > header_decoder;
        header_decoder.frame_info | frame_info.frame_sync;
        header_decoder | decoder;
    );
    Ok((frame_sync, decoder))
}

#[allow(clippy::too_many_arguments)]
pub fn build_lora_rx_hard_decoding(
    mut fg: &mut Flowgraph,
    chan: Channel,
    bw: Bandwidth,
    sf: SpreadingFactor,
    header_mode: HeaderMode,
    ldro: LdroMode,
    initial_sync_words: Option<&[SynchWord]>,
    os_factor: usize,
    preamble_len: Option<usize>,
    sync_word_caching_policy: Option<&str>,
    collect_receive_statistics: bool,
    startup_timestamp: Option<SystemTime>,
) -> Result<(BlockRef<FrameSync>, BlockRef<Decoder>)> {
    let ldro_enabled = ldro.resolve_if_auto(sf, bw).enabled();
    let frame_sync = FrameSync::new(
        chan,
        bw,
        sf,
        !matches!(header_mode, HeaderMode::Explicit),
        initial_sync_words.unwrap_or_default(),
        os_factor,
        preamble_len,
        sync_word_caching_policy,
        collect_receive_statistics,
        startup_timestamp,
    );
    let fft_demod: FftDemod<_, _, _, _> = FftDemod::<
        DemodulatedSymbolHardDecoding,
        fft_demod::State<DemodulatedSymbolHardDecoding>,
    >::new(sf, ldro_enabled);
    let gray_mapping: GrayMapping<_, _, _> = GrayMapping::<DemodulatedSymbolHardDecoding>::new();
    let deinterleaver: Deinterleaver<_, _, _, _> = Deinterleaver::<
        DemodulatedSymbolHardDecoding,
        DeinterleavedSymbolHardDecoding,
    >::new(ldro_enabled, sf);
    let hamming_dec: HammingDecoder<_, _, _> =
        HammingDecoder::<DeinterleavedSymbolHardDecoding>::new();
    let header_decoder: HeaderDecoder = HeaderDecoder::new(header_mode, ldro_enabled);
    let decoder: Decoder = Decoder::new();
    connect!(fg,
        frame_sync > fft_demod > gray_mapping > deinterleaver > hamming_dec > header_decoder;
        header_decoder.frame_info | frame_info.frame_sync;
        header_decoder | decoder;
    );
    Ok((frame_sync, decoder))
}
