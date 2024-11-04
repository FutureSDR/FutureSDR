use futuresdr::anyhow::Result;
use futuresdr::blocks::seify::{Capabilities, SourceBuilder};
use futuresdr_types::Pmt;
use seify::{Range, RangeItem};

#[test]
fn cap_pmt_serde() -> Result<()> {
    let capabilities = Capabilities {
        frequency_range: Some(Range {
            items: vec![RangeItem::Interval(100.0, 200.0)],
        }),
        sample_rate_range: Some(Range {
            items: vec![RangeItem::Step(1.0, 2.0, 3.0)],
        }),
        bandwidth_range: Some(Range {
            items: vec![RangeItem::Value(10.0)],
        }),
        antennas: Some(vec!["antenna1".to_string(), "antenna2".to_string()]),
        gain_range: Some(Range {
            items: vec![
                RangeItem::Interval(0.0, 10.0),
                RangeItem::Value(20.0),
                RangeItem::Step(30.0, 40.0, 50.0),
            ],
        }),
        supports_agc: Some(true),
    };

    let pmt = Pmt::from(&capabilities);
    let back: Capabilities = pmt.try_into()?;

    dbg!(back);

    Ok(())
}
