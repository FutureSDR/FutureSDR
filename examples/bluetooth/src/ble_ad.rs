use std::error::Error;
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BleAdStructure {
    pub ad_type: u8,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BleAdParseError {
    Truncated {
        offset: usize,
        len: usize,
        actual: usize,
    },
}

impl fmt::Display for BleAdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated {
                offset,
                len,
                actual,
            } => write!(
                f,
                "truncated AD structure at offset {offset}: need {len} bytes, got {actual}"
            ),
        }
    }
}

impl Error for BleAdParseError {}

pub fn parse_ad_structures(data: &[u8]) -> Result<Vec<BleAdStructure>, BleAdParseError> {
    let mut out = Vec::new();
    let mut offset = 0usize;

    while offset < data.len() {
        let len = data[offset] as usize;
        offset += 1;

        if len == 0 {
            break;
        }

        if offset + len > data.len() {
            return Err(BleAdParseError::Truncated {
                offset: offset - 1,
                len,
                actual: data.len() - offset,
            });
        }

        out.push(BleAdStructure {
            ad_type: data[offset],
            data: data[offset + 1..offset + len].to_vec(),
        });
        offset += len;
    }

    Ok(out)
}

pub fn format_ad_structures(ad_structures: &[BleAdStructure]) -> String {
    ad_structures
        .iter()
        .map(format_ad_structure)
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_ad_structure(ad: &BleAdStructure) -> String {
    match ad.ad_type {
        0x01 if !ad.data.is_empty() => format!("flags=0x{:02X}", ad.data[0]),
        0x02 | 0x03 => format_uuid16_list("uuid16", &ad.data),
        0x08 => format!("short_name=\"{}\"", String::from_utf8_lossy(&ad.data)),
        0x09 => format!("name=\"{}\"", String::from_utf8_lossy(&ad.data)),
        0x0a if !ad.data.is_empty() => format!("tx_power={}dBm", ad.data[0] as i8),
        0x16 if ad.data.len() >= 2 => {
            let uuid = u16::from_le_bytes([ad.data[0], ad.data[1]]);
            format!(
                "service_data=0x{uuid:04X}/{}B",
                ad.data.len().saturating_sub(2)
            )
        }
        0xff if ad.data.len() >= 2 => {
            let company = u16::from_le_bytes([ad.data[0], ad.data[1]]);
            format!(
                "mfg={}({company:#06X})/{}B",
                company_name(company),
                ad.data.len().saturating_sub(2)
            )
        }
        _ => format!("type=0x{:02X}/{}B", ad.ad_type, ad.data.len()),
    }
}

fn format_uuid16_list(label: &str, data: &[u8]) -> String {
    let values = data
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .map(|uuid| format!("0x{uuid:04X}"))
        .collect::<Vec<_>>()
        .join("|");
    format!("{label}=[{values}]")
}

fn company_name(company: u16) -> &'static str {
    match company {
        0x0006 => "Microsoft",
        0x004c => "Apple",
        0x0075 => "Samsung",
        0x00e0 => "Google",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ad_structures() {
        let data = [
            0x02, 0x01, 0x06, 0x05, 0x09, b'F', b'S', b'D', b'R', 0x04, 0xff, 0x4c, 0x00, 0x01,
        ];
        let ad = parse_ad_structures(&data).unwrap();

        assert_eq!(
            ad,
            vec![
                BleAdStructure {
                    ad_type: 0x01,
                    data: vec![0x06],
                },
                BleAdStructure {
                    ad_type: 0x09,
                    data: b"FSDR".to_vec(),
                },
                BleAdStructure {
                    ad_type: 0xff,
                    data: vec![0x4c, 0x00, 0x01],
                },
            ]
        );
        assert_eq!(
            format_ad_structures(&ad),
            "flags=0x06, name=\"FSDR\", mfg=Apple(0x004C)/1B"
        );
    }
}
