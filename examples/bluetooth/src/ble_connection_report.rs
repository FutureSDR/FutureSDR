#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ConnectionTrackingSummary {
    pub(crate) connect_ind_seen: u64,
    pub(crate) connections: u64,
    pub(crate) locked_connections: u64,
    pub(crate) searching_connections: u64,
    pub(crate) discontinuous_connections: u64,
    pub(crate) data_packets: u64,
    pub(crate) overflow_events: u64,
    pub(crate) relocks: u64,
    pub(crate) recovery_observations: u64,
    pub(crate) recovery_resets: u64,
    pub(crate) recovery_candidates: u64,
    pub(crate) expired_connections: u64,
    pub(crate) timing_sample_rate_misses: u64,
    pub(crate) timing_invalid_parameter_misses: u64,
    pub(crate) timing_before_window_misses: u64,
    pub(crate) timing_outside_window_misses: u64,
    pub(crate) timing_channel_misses: u64,
    pub(crate) control_pdus: u64,
    pub(crate) control_parse_errors: u64,
    pub(crate) unresolved_control_updates: u64,
    pub(crate) connection_updates_scheduled: u64,
    pub(crate) connection_updates_applied: u64,
    pub(crate) channel_map_updates_scheduled: u64,
    pub(crate) channel_map_updates_applied: u64,
    pub(crate) pending_connection_updates: u64,
    pub(crate) pending_channel_map_updates: u64,
    pub(crate) terminated_connections: u64,
    pub(crate) encryption_starts: u64,
    pub(crate) encrypted_connections: u64,
}

pub(crate) fn format_control_summary(summary: ConnectionTrackingSummary) -> Option<String> {
    (summary.control_pdus > 0
        || summary.control_parse_errors > 0
        || summary.unresolved_control_updates > 0
        || summary.connection_updates_scheduled > 0
        || summary.channel_map_updates_scheduled > 0
        || summary.terminated_connections > 0
        || summary.encryption_starts > 0)
        .then(|| {
            format!(
                "pdus={} connection_updates={}/{} channel_map_updates={}/{} pending={}/{} terminations={} encryption_starts={} encrypted_connections={} unresolved_updates={} parse_errors={}",
                summary.control_pdus,
                summary.connection_updates_applied,
                summary.connection_updates_scheduled,
                summary.channel_map_updates_applied,
                summary.channel_map_updates_scheduled,
                summary.pending_connection_updates,
                summary.pending_channel_map_updates,
                summary.terminated_connections,
                summary.encryption_starts,
                summary.encrypted_connections,
                summary.unresolved_control_updates,
                summary.control_parse_errors,
            )
        })
}

pub(crate) fn format_tracking_summary(summary: ConnectionTrackingSummary) -> String {
    let timing_misses = summary.timing_sample_rate_misses
        + summary.timing_invalid_parameter_misses
        + summary.timing_before_window_misses
        + summary.timing_outside_window_misses
        + summary.timing_channel_misses;
    let mut reasons = Vec::new();
    append_nonzero_reason(
        &mut reasons,
        "sample_rate",
        summary.timing_sample_rate_misses,
    );
    append_nonzero_reason(
        &mut reasons,
        "invalid_parameters",
        summary.timing_invalid_parameter_misses,
    );
    append_nonzero_reason(
        &mut reasons,
        "before_window",
        summary.timing_before_window_misses,
    );
    append_nonzero_reason(
        &mut reasons,
        "outside_window",
        summary.timing_outside_window_misses,
    );
    append_nonzero_reason(&mut reasons, "channel", summary.timing_channel_misses);
    let timing = if reasons.is_empty() {
        timing_misses.to_string()
    } else {
        format!("{}({})", timing_misses, reasons.join(","))
    };
    format!(
        "connect_ind={} connections={} expired_connections={} locked={} searching={} discontinuous={} tracked_data_packets={} rx_overflows={} relocks={} recovery_observations={} recovery_resets={} recovery_candidates={} timing_misses={}",
        summary.connect_ind_seen,
        summary.connections,
        summary.expired_connections,
        summary.locked_connections,
        summary.searching_connections,
        summary.discontinuous_connections,
        summary.data_packets,
        summary.overflow_events,
        summary.relocks,
        summary.recovery_observations,
        summary.recovery_resets,
        summary.recovery_candidates,
        timing,
    )
}

fn append_nonzero_reason(reasons: &mut Vec<String>, name: &str, value: u64) {
    if value > 0 {
        reasons.push(format!("{name}={value}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_summary_formats_only_nonzero_miss_reasons() {
        let summary = ConnectionTrackingSummary {
            timing_outside_window_misses: 1,
            timing_channel_misses: 83,
            ..ConnectionTrackingSummary::default()
        };

        assert_eq!(
            format_tracking_summary(summary),
            "connect_ind=0 connections=0 expired_connections=0 locked=0 searching=0 discontinuous=0 tracked_data_packets=0 rx_overflows=0 relocks=0 recovery_observations=0 recovery_resets=0 recovery_candidates=0 timing_misses=84(outside_window=1,channel=83)"
        );
    }
}
