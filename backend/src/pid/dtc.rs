//! Decode DTC pairs within message boundaries, never by splitting on a byte in a code.
//! ELM327 datasheet, "Interpreting Trouble Codes" and "CAN Message Types":
//! https://www.elmelectronics.com/wp-content/uploads/2017/01/ELM327DS.pdf
use super::diagnostics::{TroubleCode, TroubleCodeCategory};
use crate::Error;
use std::collections::{BTreeMap, BTreeSet};

struct Assembly {
    data: Vec<u8>,
    expected: usize,
    next: u8,
}
pub(super) fn decode_response(raw: &str, service: u8) -> Result<Vec<TroubleCode>, Error> {
    if raw.len() > 65536 {
        return Err(Error::InvalidResponse);
    }
    let mut messages: Vec<Vec<u8>> = Vec::new();
    let mut frames: BTreeMap<String, Assembly> = BTreeMap::new();
    for line in raw.split(['\r', '\n']) {
        let line = line.trim().trim_end_matches('>').trim();
        if line.is_empty()
            || line == "SEARCHING..."
            || line.eq_ignore_ascii_case(&format!("{:02X}", service - 0x40))
        {
            continue;
        }
        let line = line.strip_prefix("SEARCHING...").unwrap_or(line).trim();
        if line.replace(' ', "").eq_ignore_ascii_case("NODATA") {
            return Err(Error::NoData);
        }
        let tokens: Vec<_> = line.split_whitespace().collect();
        let compact_header = tokens.first().filter(|s| {
            (s.len() == 3 || s.len() == 8)
                && s.bytes().all(|b| b.is_ascii_hexdigit())
                && tokens.len() > 1
        });
        let split_can_header = tokens.len() > 4
            && tokens[..4]
                .iter()
                .all(|s| s.len() == 2 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            && u8::from_str_radix(tokens[0], 16).is_ok_and(|b| b <= 0x1F);
        let (header, header_tokens) = if let Some(header) = compact_header {
            (Some(header.to_string()), 1)
        } else if split_can_header {
            (Some(tokens[..4].join("")), 4)
        } else {
            (None, 0)
        };
        let bytes_text = if header.is_some() {
            tokens[header_tokens..].join("")
        } else {
            tokens.join("")
        };
        if bytes_text.is_empty() || bytes_text.len() % 2 != 0 || !bytes_text.is_ascii() {
            return Err(Error::InvalidResponse);
        }
        let bytes: Vec<u8> = bytes_text
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                u8::from_str_radix(std::str::from_utf8(pair).unwrap_or(""), 16)
                    .map_err(|_| Error::InvalidResponse)
            })
            .collect::<Result<_, _>>()?;
        if let Some(header) = header {
            match bytes[0] >> 4 {
                0 => {
                    let length = (bytes[0] & 15) as usize;
                    if length == 0 || bytes.len() < length + 1 {
                        return Err(Error::InvalidResponse);
                    }
                    messages.push(bytes[1..=length].to_vec());
                }
                1 => {
                    if bytes.len() < 3 {
                        return Err(Error::InvalidResponse);
                    }
                    let expected = ((bytes[0] as usize & 15) << 8) | bytes[1] as usize;
                    if expected <= bytes.len() - 2 || frames.contains_key(&header) {
                        return Err(Error::InvalidResponse);
                    }
                    frames.insert(
                        header.to_string(),
                        Assembly {
                            data: bytes[2..].to_vec(),
                            expected,
                            next: 1,
                        },
                    );
                }
                2 => {
                    let frame = frames.get_mut(&header).ok_or(Error::InvalidResponse)?;
                    if bytes[0] & 15 != frame.next {
                        return Err(Error::InvalidResponse);
                    }
                    frame.next = (frame.next + 1) & 15;
                    let remaining = frame.expected.saturating_sub(frame.data.len());
                    if remaining == 0 {
                        return Err(Error::InvalidResponse);
                    }
                    frame.data.extend(bytes[1..].iter().take(remaining));
                }
                _ => return Err(Error::InvalidResponse),
            }
        } else if bytes.len() > 4 && bytes[3] == service && bytes[0] != service {
            // Legacy ATH1 output has three header bytes and may include a checksum.
            // ISO 9141/KWP uses an additive checksum; SAE J1850 uses CRC-8.
            let payload_end = if (bytes.len() - 4) % 2 == 1 {
                let end = bytes.len() - 1;
                let sum = bytes[..end]
                    .iter()
                    .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
                if bytes[end] != sum && bytes[end] != j1850_checksum(&bytes[..end]) {
                    return Err(Error::InvalidResponse);
                }
                end
            } else {
                bytes.len()
            };
            messages.push(bytes[3..payload_end].to_vec());
        } else {
            // Headerless ELM output is already formatted as service + DTC data.
            if bytes[0] != service {
                return Err(Error::InvalidResponse);
            }
            messages.push(bytes);
        }
    }
    for frame in frames.into_values() {
        if frame.data.len() != frame.expected {
            return Err(Error::InvalidResponse);
        }
        messages.push(frame.data);
    }
    if messages.is_empty() {
        return Err(Error::NoData);
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for message in messages {
        if message.first() != Some(&service) {
            return Err(Error::InvalidResponse);
        }
        let body = &message[1..];
        if body.is_empty() {
            return Err(Error::InvalidResponse);
        }
        // CAN includes a count byte. Legacy payloads have an even number of DTC bytes.
        let data = if body.len() % 2 == 1 {
            let length = body[0] as usize * 2;
            if body.len() < length + 1 || body[length + 1..].iter().any(|b| *b != 0) {
                return Err(Error::InvalidResponse);
            }
            &body[1..length + 1]
        } else {
            body
        };
        let mut padding = false;
        for pair in data.chunks_exact(2) {
            if pair == [0, 0] {
                padding = true;
                continue;
            }
            if padding {
                return Err(Error::InvalidResponse);
            }
            let category = match pair[0] >> 6 {
                0 => TroubleCodeCategory::Powertrain,
                1 => TroubleCodeCategory::Chassis,
                2 => TroubleCodeCategory::Body,
                _ => TroubleCodeCategory::Network,
            };
            let name = format!(
                "{}{:X}{:X}{:02X}",
                category.system_letter(),
                (pair[0] >> 4) & 3,
                pair[0] & 15,
                pair[1]
            );
            if seen.insert(name.clone()) {
                out.push(TroubleCode::new(category, name, service == 0x4A));
            }
        }
    }
    Ok(out)
}

fn j1850_checksum(bytes: &[u8]) -> u8 {
    let mut crc = 0xFFu8;
    for byte in bytes {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x1D
            } else {
                crc << 1
            };
        }
    }
    crc ^ 0xFF
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_multiframe_codes_and_incomplete_frames() {
        let raw = "7E8 10 0A 43 04 01 00 01 43\r7E8 21 01 33 C0 16 00 00 00\r>";
        let codes = decode_response(raw, 0x43).unwrap();
        assert_eq!(codes.len(), 4);
        assert!(codes.iter().any(|c| c.dtc == "P0143"));
        assert!(decode_response("7E8 10 0A 43 04 01 00 01 43", 0x43).is_err());
        assert!(decode_response("NO DATA", 0x43).is_err());
        assert!(decode_response("43 GG 00", 0x43).is_err());
    }
    #[test]
    fn legacy_checksums_are_validated() {
        assert!(decode_response("48 6B 10 43 01 33 00 00 00 00 3B", 0x43).is_err());
        assert_eq!(j1850_checksum(b"123456789"), 0x4B);
        let mut frame = vec![0x48, 0x6B, 0x10, 0x43, 0x01, 0x33];
        frame.push(j1850_checksum(&frame));
        let raw = frame
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(decode_response(&raw, 0x43).unwrap()[0].dtc, "P0133");
    }
}
