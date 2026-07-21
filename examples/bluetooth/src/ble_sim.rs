use futuresdr::num_complex::Complex32;

pub fn generate_ble_packet_samples(
    samples_per_symbol: usize,
    packet_count: usize,
    channel_index: u8,
) -> Vec<Complex32> {
    let bits = super::ble_protocol::generate_ble_packet_bits(channel_index);
    let startup_silence = samples_per_symbol * 600;
    let packet_gap = samples_per_symbol * 100;
    let mut samples = Vec::with_capacity(
        startup_silence + (bits.len() * samples_per_symbol + packet_gap) * packet_count,
    );

    for _ in 0..startup_silence {
        samples.push(Complex32::new(0.0, 0.0));
    }

    let mut current_phase = 0.0f32;
    let phase_step = (std::f32::consts::PI * 0.5) / samples_per_symbol as f32;

    for _ in 0..packet_count {
        for &bit in &bits {
            let symbol = if bit == 1 { 1.0 } else { -1.0 };
            for _ in 0..samples_per_symbol {
                current_phase += symbol * phase_step;

                while current_phase <= -std::f32::consts::PI {
                    current_phase += 2.0 * std::f32::consts::PI;
                }
                while current_phase > std::f32::consts::PI {
                    current_phase -= 2.0 * std::f32::consts::PI;
                }

                samples.push(Complex32::from_polar(1.0, current_phase));
            }
        }

        for _ in 0..packet_gap {
            samples.push(Complex32::new(0.0, 0.0));
        }
    }

    samples
}
