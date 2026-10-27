//! PBUK/DBLB Parser
//!
//! Current SWTOR format (2024+):
//!
//! PBUK Container:
//!   - bytes 0-3: "PBUK" magic
//!   - bytes 4-5: chunk count (u16) - typically 2
//!   - bytes 6-7: unknown (u16)
//!   - bytes 8-11: offset to first DBLB (always 12)
//!   - byte 12+: DBLB wrapper followed by object DBLB
//!
//! DBLB Wrapper (16 bytes, at offset 12):
//!   - bytes 0-3: "DBLB" magic
//!   - bytes 4-7: version (u32, typically 2)
//!   - bytes 8-11: padding (zeros)
//!   - bytes 12-15: total DBLB size
//!
//! Object DBLB (at offset 28):
//!   - bytes 0-3: "DBLB" magic
//!   - bytes 4-7: version (u32)
//!   - bytes 8-11: first object size (u32)
//!   - bytes 12-15: padding
//!   - byte 16+: objects
//!
//! Object format:
//!   - 42-byte header (contains GUIDs, offsets)
//!   - FQN string (null-terminated)
//!   - padding to align
//!   - ZSTD-compressed payload (trim last 8 bytes), or the raw payload when
//!     header byte 8 is `0x0E` (see `uncompressed_payload`)
//!   - 8-byte footer (next object link)
//!
//! The payload contains binary GOM data with length-prefixed strings.

use anyhow::{bail, Context, Result};

const PBUK_MAGIC: [u8; 4] = *b"PBUK";
const DBLB_MAGIC: [u8; 4] = *b"DBLB";
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

// Object size bounds based on observed SWTOR data
const MIN_OBJECT_SIZE: usize = 50;
const MAX_OBJECT_SIZE: usize = 50000;
// Safety limit to prevent infinite loops
const MAX_OBJECTS_PER_DBLB: usize = 10000;

/// Check if data starts with PBUK magic
pub fn is_pbuk(data: &[u8]) -> bool {
    data.len() >= 4 && data[0..4] == PBUK_MAGIC
}

/// Check if data starts with DBLB magic
pub fn is_dblb(data: &[u8]) -> bool {
    data.len() >= 4 && data[0..4] == DBLB_MAGIC
}

/// A parsed GOM object from DBLB
#[derive(Debug, Clone)]
pub struct GomObject {
    /// Fully Qualified Name (e.g., "itm.gen.lots.weapon...")
    pub fqn: String,
    /// Raw header bytes (42 bytes, contains GUIDs)
    pub header: Vec<u8>,
    /// Decompressed GOM payload (binary format)
    pub payload: Vec<u8>,
}

/// Extract ASCII strings from a raw GOM payload.
///
/// GOM uses at least three string encodings empirically:
///
/// 1. Canonical: `0x06 <len> <ASCII>` -- common in spawn payloads.
/// 2. With prefix: `0x06 <flag bytes> <len> <ASCII>` -- common in quest payloads
///    (e.g. `0x06 01 01 01 a7 ...`). Intermediate bytes are array-count or
///    flag metadata between the marker and the actual length.
/// 3. Array element: `0xD2 0x01 <index> <len> <ASCII>` -- common in encounter
///    payloads. `0xD2 0x01` is an array-element header followed by an
///    incrementing 1-byte index ('A', 'B', 'C', ...) before the length.
///
/// We try in priority: array element first (strictest signature), canonical
/// 0x06 next, and a bare-length fallback last. The fallback recovers strings
/// whose marker is followed by intermediate bytes by landing on the actual
/// length byte once the more specific patterns walk past.
///
/// This is heuristic, not a full msgpack-style decode. If a future format
/// change introduces ambiguity, the right fix is to decode the type-tag
/// stream properly rather than to refine the heuristics further.
pub fn extract_strings_from_payload(payload: &[u8]) -> Vec<String> {
    let mut strings = Vec::new();
    let mut i = 0;

    while i + 4 <= payload.len() {
        // Pattern 3: array element `0xD2 0x01 <index> <len> <ASCII>`.
        if payload[i] == 0xD2 && payload[i + 1] == 0x01 {
            let len = payload[i + 3] as usize;
            let start = i + 4;
            if (2..200).contains(&len) && start + len <= payload.len() {
                let candidate = &payload[start..start + len];
                if candidate.iter().all(|&b| (32..127).contains(&b)) {
                    if let Ok(s) = std::str::from_utf8(candidate) {
                        strings.push(s.to_string());
                        i = start + len;
                        continue;
                    }
                }
            }
            i += 1;
            continue;
        }

        // Pattern 1: canonical `0x06 <len> <ASCII>`.
        if payload[i] == 0x06 {
            let len = payload[i + 1] as usize;
            let start = i + 2;
            if (2..200).contains(&len) && start + len <= payload.len() {
                let candidate = &payload[start..start + len];
                if candidate.iter().all(|&b| (32..127).contains(&b)) {
                    if let Ok(s) = std::str::from_utf8(candidate) {
                        strings.push(s.to_string());
                        i = start + len;
                        continue;
                    }
                }
            }
            i += 1;
            continue;
        }

        // Fallback: `<len> <ASCII>` directly. Catches pattern 2 by landing
        // on the actual length byte once intermediates have been skipped.
        let len = payload[i] as usize;
        let start = i + 1;
        if (2..200).contains(&len) && start + len <= payload.len() {
            let candidate = &payload[start..start + len];
            if candidate.iter().all(|&b| (32..127).contains(&b)) {
                if let Ok(s) = std::str::from_utf8(candidate) {
                    strings.push(s.to_string());
                    i = start + len;
                    continue;
                }
            }
        }
        i += 1;
    }

    strings
}

impl GomObject {
    /// Try to extract strings from the binary payload
    pub fn extract_strings(&self) -> Vec<String> {
        extract_strings_from_payload(&self.payload)
    }
}

/// Parse a PBUK container, extracting all GOM objects
pub fn parse(data: &[u8]) -> Result<Vec<GomObject>> {
    if !is_pbuk(data) {
        bail!("Not a PBUK file");
    }

    // PBUK structure:
    // - 12 byte header
    // - 16 byte DBLB wrapper (at offset 12)
    // - Object DBLB (at offset 28)

    if data.len() < 44 {
        bail!("PBUK too small");
    }

    // Verify DBLB wrapper at offset 12
    if data[12..16] != DBLB_MAGIC {
        bail!("No DBLB wrapper at offset 12");
    }

    // Verify object DBLB at offset 28
    if data[28..32] != DBLB_MAGIC {
        bail!("No object DBLB at offset 28");
    }

    // Parse object DBLB
    let objects_dblb = &data[28..];
    parse_object_dblb(objects_dblb)
}

/// True for bytes that can appear inside a GOM FQN string. Most FQNs are
/// dot-separated lowercase identifiers like `tal.spvp.engine.barrel_roll.tier1`,
/// but PBUK singleton prototype names use PascalCase / camelCase
/// (`tagTablePrototype`, `colCollectionItemsPrototype`, `Suburb`, `Federation`),
/// so both cases must be accepted.
fn is_fqn_byte(b: u8) -> bool {
    b.is_ascii_alphabetic() || b.is_ascii_digit() || b == b'.' || b == b'_'
}

/// Maximum number of bytes the walkback can shift. Bounded to prevent runaway
/// scans if a preamble happens to end with fqn-shaped bytes, but large enough
/// to cover the longest singleton FQNs observed in the corpus
/// (`colCollectionItemsPrototype` = 27 chars + a few bytes for the preamble
/// terminator).
const WALKBACK_MAX_STEPS: usize = 64;

/// Number of bytes the nominal FQN offset (`object_start + 42`) overshot the
/// real FQN start. The GOM "header" is mostly non-ASCII; if the byte right
/// before `object_start + 42` is a valid FQN byte, the preamble is shorter
/// than 42 bytes and the parser must shift the object slice backward by this
/// amount so the GUID/header bytes also come out right.
fn walkback_amount(data: &[u8], object_start: usize) -> usize {
    let nominal = object_start + 42;
    if nominal >= data.len() {
        return 0;
    }
    let mut steps = 0;
    while steps < WALKBACK_MAX_STEPS && nominal > steps && is_fqn_byte(data[nominal - 1 - steps]) {
        steps += 1;
    }
    steps
}

/// Longest decimal guid string seen between an FQN and its payload (a u64 has
/// at most 20 decimal digits).
const GUID_STRING_MAX_DIGITS: usize = 20;

/// Offset of the zstd payload that follows an FQN ending at `fqn_end` (the
/// FQN's NUL terminator), or `None`.
///
/// Most objects put the payload right after the FQN. A second layout, used by
/// most quests (`qst.*`) and some emotes, first writes the object's guid as a
/// decimal ASCII string with its own NUL (`<fqn>\0<digits>\0<zstd>`); that
/// string is skipped before the usual short search for the zstd magic.
fn payload_start(data: &[u8], fqn_end: usize) -> Option<usize> {
    let mut from = fqn_end;
    let digits_start = fqn_end + 1;
    let digits = data
        .get(digits_start..)?
        .iter()
        .take(GUID_STRING_MAX_DIGITS + 1)
        .take_while(|b| b.is_ascii_digit())
        .count();
    if (1..=GUID_STRING_MAX_DIGITS).contains(&digits) && data.get(digits_start + digits) == Some(&0)
    {
        from = digits_start + digits;
    }
    (from..data.len().min(from + 10)).find(|&i| data.len() > i + 4 && data[i..i + 4] == ZSTD_MAGIC)
}

/// Header byte 8 of an object whose payload is stored without compression
/// (`0x0F` marks a zstd payload). Item appearances (`ipp.*`), `cos`, `dec`,
/// `pth`, `phs` and others use this layout: after the FQN's NUL come zero
/// padding bytes and the raw payload, no zstd frame.
const UNCOMPRESSED_FLAG: u8 = 0x0E;

/// The raw payload of an object stored uncompressed, given the offset of its
/// 42-byte header in `data`, or `None` when the object is not stored that way.
///
/// The header's offsets count from 8 bytes before the header: the u16 at
/// byte 10 is where the payload region starts (just past the FQN's NUL), and
/// the u16 at byte 30 plus the u32 at byte 32 is where it ends. A zstd
/// payload decompresses to the same region, padding included, so both
/// layouts hand the same bytes to the GOM reader.
fn uncompressed_payload(data: &[u8], header_start: usize) -> Option<(usize, usize)> {
    let header = data.get(header_start..header_start + 42)?;
    if header[8] != UNCOMPRESSED_FLAG {
        return None;
    }
    let u16_at = |i: usize| usize::from(u16::from_le_bytes([header[i], header[i + 1]]));
    let size = u32::from_le_bytes([header[32], header[33], header[34], header[35]]);
    let start = (header_start + u16_at(10)).checked_sub(8)?;
    let end = (header_start + u16_at(30) + usize::try_from(size).ok()?).checked_sub(8)?;
    (header_start + 42 < start && start <= end && end <= data.len()).then_some((start, end))
}

/// Offset of a zstd payload found where the header says it starts (the u16
/// at header byte 10, counted from 8 bytes before the header). Used when the
/// payload is not right after the FQN, as in objects that store a text string
/// between the two.
fn compressed_start(data: &[u8], header_start: usize) -> Option<usize> {
    let header = data.get(header_start..header_start + 42)?;
    let offset = usize::from(u16::from_le_bytes([header[10], header[11]]));
    let start = (header_start + offset).checked_sub(8)?;
    (header_start + 42 < start && data.get(start..start + 4) == Some(&ZSTD_MAGIC[..]))
        .then_some(start)
}

/// Try to extract next object size from the 8-byte footer.
/// SWTOR stores the size at varying positions in the footer.
fn extract_next_size_from_footer(footer: &[u8]) -> Option<usize> {
    if footer.len() != 8 {
        return None;
    }

    // Find first non-zero byte and read size from there
    let first_nonzero = footer.iter().position(|&b| b != 0)?;

    // Try reading as u16 LE
    if first_nonzero + 2 <= 8 {
        let val = u16::from_le_bytes([footer[first_nonzero], footer[first_nonzero + 1]]) as usize;
        if val > MIN_OBJECT_SIZE && val < MAX_OBJECT_SIZE {
            return Some(val);
        }
    }

    // Single byte fallback
    let val = footer[first_nonzero] as usize;
    if val > MIN_OBJECT_SIZE {
        Some(val)
    } else {
        None
    }
}

/// Parse the object DBLB block using a hybrid approach:
/// 1. Use footer chain for fast parsing (no ZSTD probing)
/// 2. Fall back to ZSTD probing when footer chain breaks
fn parse_object_dblb(data: &[u8]) -> Result<Vec<GomObject>> {
    if !is_dblb(data) {
        bail!("Not a DBLB block");
    }

    let mut objects = Vec::new();

    // DBLB header: 16 bytes, first object size at bytes 8-11
    let first_obj_size = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;

    tracing::debug!("Object DBLB: first object size = {}", first_obj_size);

    let mut offset = 16;
    let mut obj_size = first_obj_size;
    let mut using_footer_chain = true;

    while offset + MIN_OBJECT_SIZE < data.len() && objects.len() < MAX_OBJECTS_PER_DBLB {
        if using_footer_chain && obj_size > MIN_OBJECT_SIZE && offset + obj_size <= data.len() {
            // Fast path: parse using known object size from footer chain
            let obj_data = &data[offset..offset + obj_size];

            // Validate: check FQN looks valid
            let fqn_valid =
                obj_data.len() > 46 && obj_data[42..46].iter().all(|&b| (32..127).contains(&b));

            if fqn_valid {
                // Get footer for next iteration BEFORE trying to parse
                let footer = &obj_data[obj_data.len() - 8..];
                let next_size = extract_next_size_from_footer(footer);

                // A subset of objects (~50 GSF talents + others) has a
                // variable-length preamble that shifts the real object start
                // earlier than the footer-chain offset suggests. Detect by
                // walking back through the contiguous ASCII-FQN run from
                // offset+42 within the FULL data buffer; if the FQN actually
                // starts earlier, shift the slice so the GUID is read from
                // the right bytes too.
                let walkback = walkback_amount(data, offset);
                let parse_result = if walkback > 0 && offset >= walkback {
                    let shifted = &data[offset - walkback..offset + obj_size - walkback];
                    parse_object(shifted)
                } else {
                    parse_object(obj_data)
                };
                let Ok(obj) = parse_result else {
                    // The footer's size can be wrong for an object (a large
                    // payload, or text between the FQN and the payload), and a
                    // slice of the wrong size does not parse. Let the scanning
                    // path below read this same offset with the payload's own
                    // frame size rather than skipping the object.
                    using_footer_chain = false;
                    continue;
                };
                objects.push(obj);

                // Move to next object
                let next_unaligned = offset + obj_size;
                offset = if !next_unaligned.is_multiple_of(8) {
                    next_unaligned + (8 - next_unaligned % 8)
                } else {
                    next_unaligned
                };

                match next_size {
                    Some(s) if s > MIN_OBJECT_SIZE && s < MAX_OBJECT_SIZE => {
                        obj_size = s;
                        continue;
                    }
                    _ => {
                        // Footer chain broken, switch to scanning
                        using_footer_chain = false;
                    }
                }
            } else {
                using_footer_chain = false;
            }
        }

        // Slow path: scan for objects by FQN pattern
        let nominal_fqn = offset + 42;
        if nominal_fqn + 4 >= data.len() {
            break;
        }

        let potential_fqn = &data[nominal_fqn..data.len().min(nominal_fqn + 4)];
        let has_fqn = potential_fqn.iter().all(|&b| (32..127).contains(&b));

        if !has_fqn {
            offset += 8;
            continue;
        }

        // Walk backward through the contiguous ASCII-FQN run to find the true
        // FQN start (preamble can be shorter than 42 bytes for some objects),
        // and shift the apparent object start by the same amount so the
        // header/GUID bytes come from the right position too.
        let walkback = walkback_amount(data, offset);
        let real_offset = offset.saturating_sub(walkback);
        let fqn_pos = real_offset + 42;

        // Find end of FQN
        let mut fqn_end = fqn_pos;
        while fqn_end < data.len() && data[fqn_end] != 0 {
            fqn_end += 1;
        }

        if let Some((start, end)) = uncompressed_payload(data, real_offset) {
            objects.push(GomObject {
                fqn: String::from_utf8_lossy(&data[fqn_pos..fqn_end]).to_string(),
                header: data[real_offset..real_offset + 42].to_vec(),
                payload: data[start..end].to_vec(),
            });
            // Always move forward, even if a corrupt header gives a small end.
            offset = (end + 8).next_multiple_of(8).max(offset + 8);
        } else if let Some(zstd_start) =
            payload_start(data, fqn_end).or_else(|| compressed_start(data, real_offset))
        {
            // Use ZSTD's frame size detection - O(1) instead of probing
            let zstd_data = &data[zstd_start..];
            if let Ok(frame_size) = zstd_safe::find_frame_compressed_size(zstd_data) {
                if frame_size > 0 && zstd_start + frame_size <= data.len() {
                    if let Ok(decoded) = zstd::decode_all(&zstd_data[..frame_size]) {
                        let obj_end = zstd_start + frame_size + 8;
                        let fqn = String::from_utf8_lossy(&data[fqn_pos..fqn_end]).to_string();
                        let header = data
                            [real_offset..real_offset.saturating_add(42).min(data.len())]
                            .to_vec();

                        objects.push(GomObject {
                            fqn,
                            header,
                            payload: decoded,
                        });

                        offset = obj_end;
                        if offset % 8 != 0 {
                            offset += 8 - (offset % 8);
                        }
                    } else {
                        offset = fqn_end + 8;
                    }
                } else {
                    offset = fqn_end + 8;
                }
            } else {
                offset = fqn_end + 8;
            }
        } else {
            offset = fqn_end + 8;
        }
    }

    // Recovery pass: the footer chain occasionally jumps over a real object
    // (the chained next-size lands the cursor past an object boundary
    // entirely, rather than the off-by-N alignment that walkback handles).
    // Scan for `01 02 <FQN>` markers (`01 03 <FQN>` in the guid-string layout)
    // in the buffer and attempt to parse any FQN we haven't already produced.
    // Bounded by the nominal preamble lookup so we don't pick up spurious
    // byte coincidences elsewhere in payloads.
    let parsed_fqns: std::collections::HashSet<String> =
        objects.iter().map(|o| o.fqn.clone()).collect();
    let mut i = 42;
    while i + 6 < data.len() {
        if data[i - 1] == 0x01 && matches!(data[i], 0x02 | 0x03) && is_fqn_byte(data[i + 1]) {
            // Found an `01 02|03 <fqn-byte>` triple. Walk forward for the FQN.
            let fqn_start = i + 1;
            let mut fqn_end = fqn_start;
            while fqn_end < data.len() && fqn_end < fqn_start + 200 && data[fqn_end] != 0 {
                fqn_end += 1;
            }
            if fqn_end > fqn_start && fqn_end < data.len() {
                let candidate_fqn = String::from_utf8_lossy(&data[fqn_start..fqn_end]).to_string();
                if !parsed_fqns.contains(&candidate_fqn)
                    && candidate_fqn.contains('.')
                    && fqn_start >= 42
                {
                    // Attempt parse: object starts 42 bytes before FQN.
                    let obj_start = fqn_start - 42;
                    if let Some((start, end)) = uncompressed_payload(data, obj_start) {
                        objects.push(GomObject {
                            fqn: candidate_fqn,
                            header: data[obj_start..fqn_start].to_vec(),
                            payload: data[start..end].to_vec(),
                        });
                        i = end;
                        continue;
                    }
                    // Need a payload bound. Find next ZSTD magic and frame size.
                    if let Some(zs) =
                        payload_start(data, fqn_end).or_else(|| compressed_start(data, obj_start))
                    {
                        if let Ok(frame_size) = zstd_safe::find_frame_compressed_size(&data[zs..]) {
                            if frame_size > 0 && zs + frame_size <= data.len() {
                                if let Ok(decoded) = zstd::decode_all(&data[zs..zs + frame_size]) {
                                    let header = data[obj_start..obj_start + 42].to_vec();
                                    objects.push(GomObject {
                                        fqn: candidate_fqn,
                                        header,
                                        payload: decoded,
                                    });
                                    i = zs + frame_size;
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }

    tracing::debug!("Parsed {} GOM objects", objects.len());
    Ok(objects)
}

/// Parse a single object given its exact bytes
fn parse_object(data: &[u8]) -> Result<GomObject> {
    if data.len() < 50 {
        bail!("Object too small");
    }

    // Header is 42 bytes
    let header = data[0..42].to_vec();

    // Find FQN starting at offset 42
    let fqn_start = 42;
    let mut fqn_end = fqn_start;
    while fqn_end < data.len() && data[fqn_end] != 0 {
        fqn_end += 1;
    }

    let fqn = String::from_utf8_lossy(&data[fqn_start..fqn_end]).to_string();

    if let Some((start, end)) = uncompressed_payload(data, 0) {
        return Ok(GomObject {
            fqn,
            header,
            payload: data[start..end].to_vec(),
        });
    }

    let zstd_start = payload_start(data, fqn_end)
        .or_else(|| compressed_start(data, 0))
        .context("No ZSTD magic found")?;

    // ZSTD payload ends 8 bytes before object end
    if data.len() < 8 {
        bail!("Object too small for footer");
    }
    let payload_end = data.len() - 8;

    if payload_end <= zstd_start {
        bail!("Invalid payload bounds");
    }

    let zstd_payload = &data[zstd_start..payload_end];
    let payload = zstd::decode_all(zstd_payload).context("Failed to decompress ZSTD payload")?;

    Ok(GomObject {
        fqn,
        header,
        payload,
    })
}

/// Parse a standalone DBLB file (not wrapped in PBUK)
pub fn parse_dblb_direct(data: &[u8]) -> Result<Vec<GomObject>> {
    // For direct DBLB, skip the wrapper and go straight to object parsing
    if !is_dblb(data) {
        bail!("Not a DBLB block");
    }

    // Check if this is a wrapper DBLB (has another DBLB at offset 16)
    if data.len() > 20 && data[16..20] == DBLB_MAGIC {
        // Skip wrapper
        parse_object_dblb(&data[16..])
    } else {
        parse_object_dblb(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode strings the way GOM does: `0x06 <length> <ASCII>`.
    fn pack(strings: &[&str]) -> Vec<u8> {
        let mut buf = Vec::new();
        for s in strings {
            buf.push(0x06);
            buf.push(s.len() as u8);
            buf.extend_from_slice(s.as_bytes());
        }
        buf
    }

    #[test]
    fn extract_strings_finds_marker_prefixed_ascii() {
        let payload = pack(&["npc.korriban.foo", "enc.korriban.bar"]);
        let strings = extract_strings_from_payload(&payload);
        assert!(strings.contains(&"npc.korriban.foo".to_string()));
        assert!(strings.contains(&"enc.korriban.bar".to_string()));
    }

    #[test]
    fn extract_strings_skips_short_runs() {
        // Length 1 is below the minimum (2) and should be skipped, not yielded.
        let payload = pack(&["x", "npc.real"]);
        let strings = extract_strings_from_payload(&payload);
        assert!(!strings.iter().any(|s| s == "x"));
        assert!(strings.iter().any(|s| s == "npc.real"));
    }

    #[test]
    fn extract_strings_handles_a_prefix_encounter_refs() {
        // Quest payloads reference encounters with the `a:` type marker.
        // The free function returns the raw string; callers strip the prefix.
        let payload = pack(&["a:enc.korriban.tomb"]);
        let strings = extract_strings_from_payload(&payload);
        assert!(strings.iter().any(|s| s == "a:enc.korriban.tomb"));
    }

    #[test]
    fn extract_strings_uses_fallback_when_marker_is_absent() {
        // When the canonical 0x06 marker is preceded by intermediate bytes
        // before the length byte, the fallback heuristic recovers the string
        // by reading directly at the length byte.
        let mut payload = vec![0x06, 0x01, 0x01, 0x01, 0x41]; // marker + intermediates + length=65
        payload.extend_from_slice(
            b"spn.location.korriban.mob.tomb_2_marka_ragnos.mob02.standard_m_01",
        );
        let strings = extract_strings_from_payload(&payload);
        assert!(
            strings
                .iter()
                .any(|s| s.starts_with("spn.location.korriban")),
            "fallback must recover string after intermediate bytes, got {:?}",
            strings
        );
    }

    #[test]
    fn extract_strings_finds_spn_triple_in_quest_payload() {
        // Quest payloads embed NPC references inside semicolon-delimited triples:
        // `spn.X;npc.Y;<numeric_id>`. The whole triple is one string in GOM,
        // so the scanner must yield it intact for downstream parsing.
        let triple = b"spn.location.korriban.class.sith_warrior.judge_and_executioner.jailer_knash;npc.location.korriban.class.sith_warrior.judge_and_executioner.jailer_knash;291310451818496";
        let mut payload = vec![0x06, triple.len() as u8];
        payload.extend_from_slice(triple);
        let strings = extract_strings_from_payload(&payload);
        assert_eq!(strings.len(), 1);
        assert!(strings[0].starts_with("spn."));
        assert!(strings[0].contains(";npc."));
    }

    #[test]
    fn extract_strings_recognises_array_element_d2_01_marker() {
        // Encounter payloads encode array-element strings as
        // `0xD2 0x01 <index> <len> <ASCII>`. Without this case the scanner's
        // fallback heuristic produces a truncated string starting with the
        // index byte ('A'/'B'/...) misread as length.
        let mut payload = vec![0xD2, 0x01, b'A', 0x40]; // header + index='A' + len=0x40 (64)
        let content = b"spn.location.tatooine.mob.hub4.green.shared.poi00_wilderness.bnt";
        assert_eq!(content.len(), 64);
        payload.extend_from_slice(content);
        let strings = extract_strings_from_payload(&payload);
        assert_eq!(strings.len(), 1);
        assert_eq!(strings[0].as_bytes(), content);
        assert!(!strings[0].starts_with('A'), "must not include index byte");
    }

    #[test]
    fn extract_strings_finds_full_fqn_when_marker_is_a_substring_byte() {
        // The length byte 0x41 ('A') is itself printable ASCII. The previous
        // heuristic incorrectly emitted "Aspn.l" because it read the marker
        // 0x06 as length=6. With the marker scan, the full 65-char FQN is
        // emitted instead.
        let mut payload = vec![0x06, 0x41];
        payload.extend_from_slice(
            b"spn.location.korriban.mob.tomb_2_marka_ragnos.mob02.standard_m_01",
        );
        let strings = extract_strings_from_payload(&payload);
        assert_eq!(
            strings,
            vec!["spn.location.korriban.mob.tomb_2_marka_ragnos.mob02.standard_m_01".to_string()]
        );
    }

    #[test]
    fn is_fqn_byte_accepts_pascal_case_singletons() {
        // Singleton prototypes use PascalCase / camelCase: tagTablePrototype,
        // colCollectionItemsPrototype, Suburb, Federation. Uppercase letters
        // must be valid FQN bytes or walkback stops one byte too early on
        // singletons whose first letter is uppercase.
        assert!(is_fqn_byte(b'T'));
        assert!(is_fqn_byte(b'S'));
        assert!(is_fqn_byte(b'F'));
        assert!(is_fqn_byte(b't'));
        assert!(is_fqn_byte(b'.'));
        assert!(is_fqn_byte(b'_'));
        assert!(is_fqn_byte(b'0'));
        assert!(!is_fqn_byte(b' '));
        assert!(!is_fqn_byte(0));
        assert!(!is_fqn_byte(0xCF));
    }

    #[test]
    fn walkback_amount_handles_pascal_case_first_letter() {
        // Synthetic: object at offset 0, FQN "Suburb" ends exactly at the
        // nominal boundary (byte 41 = last char). walkback should return 6
        // to cover the full PascalCase name so the parser shifts the slice
        // and reads "Suburb" intact, not "uburb".
        let mut data = vec![0xCC; 36];
        data.extend_from_slice(b"Suburb"); // bytes 36..42
        data.push(0); // null at 42
        data.extend_from_slice(&[0xCC; 16]); // tail bytes

        let walkback = walkback_amount(&data, 0);
        assert_eq!(
            walkback, 6,
            "walkback must cover all 6 bytes of 'Suburb' (S+u+b+u+r+b)"
        );
    }

    #[test]
    fn walkback_amount_handles_long_singleton_fqn_past_old_cap() {
        // Synthetic: FQN "tagTablePrototype" (17 chars) ends at nominal=42.
        // Old code capped walkback at 8 steps, which chopped the FQN mid-name.
        // New cap (WALKBACK_MAX_STEPS = 64) covers the full singleton width.
        let mut data = vec![0xCC; 25];
        data.extend_from_slice(b"tagTablePrototype"); // bytes 25..42
        data.push(0); // null at 42
        data.extend_from_slice(&[0xCC; 16]);

        let walkback = walkback_amount(&data, 0);
        assert_eq!(
            walkback, 17,
            "walkback must cover all 17 bytes of 'tagTablePrototype'"
        );
    }

    #[test]
    fn walkback_amount_stops_at_non_fqn_byte() {
        // Regression: walkback must stop when it hits a non-FQN byte
        // (a null or other binary marker indicating preamble).
        let mut data = vec![0xCC; 30];
        data.push(0); // explicit terminator at byte 30 (non-fqn)
        data.extend_from_slice(b"abc");
        // Pad to ensure nominal index is in-range
        data.extend_from_slice(&[0xCC; 12]);

        let walkback = walkback_amount(&data, 0);
        // From nominal=42: byte 41=0xCC (not fqn), so walkback=0
        assert_eq!(walkback, 0);
    }

    #[test]
    fn walkback_amount_respects_max_steps_cap() {
        // Construct a buffer entirely of fqn-shaped bytes from nominal
        // backward. The cap (64) must bound the result; otherwise a
        // pathological payload could cause walkback to scan to offset 0.
        let data = vec![b'a'; 200];
        let walkback = walkback_amount(&data, 100);
        assert_eq!(walkback, WALKBACK_MAX_STEPS);
    }

    /// One object as `parse_object` sees it: 42-byte header, FQN, NUL, an
    /// optional decimal guid string with its NUL, the zstd payload, and an
    /// 8-byte footer.
    fn object(fqn: &str, guid_string: Option<&str>, payload: &[u8]) -> Vec<u8> {
        let mut data = vec![0xAB; 42];
        data.extend_from_slice(fqn.as_bytes());
        data.push(0);
        if let Some(g) = guid_string {
            data.extend_from_slice(g.as_bytes());
            data.push(0);
        }
        data.extend_from_slice(&zstd::encode_all(payload, 3).unwrap());
        data.extend_from_slice(&[0u8; 8]);
        data
    }

    #[test]
    fn parse_object_reads_the_guid_string_layout() {
        let data = object(
            "qst.location.pvp.starfighter.imperial.starfighter_introduction",
            Some("16140962172503595855"),
            b"quest payload",
        );
        let obj = parse_object(&data).unwrap();
        assert_eq!(
            obj.fqn,
            "qst.location.pvp.starfighter.imperial.starfighter_introduction"
        );
        assert_eq!(obj.payload, b"quest payload");
    }

    #[test]
    fn parse_object_still_reads_the_plain_layout() {
        let data = object("abl.sith_warrior.ravage", None, b"ability payload");
        let obj = parse_object(&data).unwrap();
        assert_eq!(obj.fqn, "abl.sith_warrior.ravage");
        assert_eq!(obj.payload, b"ability payload");
    }

    /// `ipp.mtx.season5.kreia.waist` as live 7.x bucket 0.bkt stores it:
    /// header byte 8 is `0x0E`, and the raw payload follows the FQN's NUL with
    /// no zstd frame. The last 8 bytes are the next object's link.
    const UNCOMPRESSED_IPP: [u8; 170] = [
        0xf8, 0xe0, 0x9a, 0xe7, 0xee, 0x13, 0x00, 0xe0, 0x0e, 0x00, 0x4f, 0x00, 0x32, 0x00, 0x4e,
        0x00, 0x50, 0x14, 0x1e, 0xe2, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x50, 0x00, 0x5a, 0x00, 0x00, 0x00, 0x50, 0x00, 0x05, 0x00, 0x01, 0x03, 0x69, 0x70, 0x70,
        0x2e, 0x6d, 0x74, 0x78, 0x2e, 0x73, 0x65, 0x61, 0x73, 0x6f, 0x6e, 0x35, 0x2e, 0x6b, 0x72,
        0x65, 0x69, 0x61, 0x2e, 0x77, 0x61, 0x69, 0x73, 0x74, 0x00, 0x00, 0x00, 0x11, 0x08, 0xcf,
        0x40, 0x00, 0x00, 0x00, 0x28, 0xcf, 0x66, 0x6a, 0x06, 0x1b, 0x69, 0x70, 0x70, 0x2e, 0x6d,
        0x74, 0x78, 0x2e, 0x73, 0x65, 0x61, 0x73, 0x6f, 0x6e, 0x35, 0x2e, 0x6b, 0x72, 0x65, 0x69,
        0x61, 0x2e, 0x77, 0x61, 0x69, 0x73, 0x74, 0xcc, 0x02, 0xed, 0xf2, 0x1a, 0xbc, 0x05, 0x11,
        0x01, 0x02, 0xca, 0x29, 0xa2, 0x61, 0x01, 0x02, 0xca, 0x29, 0xa2, 0x69, 0x02, 0x02, 0xca,
        0x0f, 0x44, 0xe1, 0x01, 0x02, 0xca, 0x1a, 0x00, 0xaa, 0xcb, 0xb8, 0x7e, 0x87, 0x7d, 0x02,
        0xca, 0x29, 0xa2, 0x6b, 0xcc, 0x0d, 0x3e, 0xd6, 0x32, 0xdf, 0x02, 0x03, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x74, 0x02,
    ];

    #[test]
    fn parse_object_reads_the_uncompressed_layout() {
        let obj = parse_object(&UNCOMPRESSED_IPP).unwrap();
        assert_eq!(obj.fqn, "ipp.mtx.season5.kreia.waist");
        assert_eq!(obj.payload, &UNCOMPRESSED_IPP[71..162]);
        assert!(obj
            .extract_strings()
            .contains(&"ipp.mtx.season5.kreia.waist".to_string()));
    }

    #[test]
    fn dblb_returns_an_uncompressed_object() {
        let mut dblb = DBLB_MAGIC.to_vec();
        dblb.extend_from_slice(&2u32.to_le_bytes());
        dblb.extend_from_slice(&170u32.to_le_bytes());
        dblb.extend_from_slice(&[0; 4]);
        dblb.extend_from_slice(&UNCOMPRESSED_IPP);
        let objects = parse_dblb_direct(&dblb).unwrap();
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].fqn, "ipp.mtx.season5.kreia.waist");
        assert_eq!(objects[0].header, &UNCOMPRESSED_IPP[..42]);
        assert_eq!(objects[0].payload, &UNCOMPRESSED_IPP[71..162]);
    }

    #[test]
    fn uncompressed_payload_ignores_a_compressed_object() {
        let data = object("abl.sith_warrior.ravage", None, b"ability payload");
        assert_eq!(data[8], 0xAB);
        assert_eq!(uncompressed_payload(&data, 0), None);
        let mut flagged = UNCOMPRESSED_IPP;
        flagged[8] = 0x0F;
        assert_eq!(uncompressed_payload(&flagged, 0), None);
    }

    #[test]
    fn payload_start_ignores_digits_without_a_terminating_nul() {
        // Digits that run straight into the payload are not a guid string, so
        // the search stays in the plain window and misses magic past it.
        let mut data = b"name\x00123456789012".to_vec();
        data.extend_from_slice(&ZSTD_MAGIC);
        data.extend_from_slice(&[0; 8]);
        assert_eq!(payload_start(&data, 4), None);
        let mut plain = b"name\x00".to_vec();
        plain.extend_from_slice(&ZSTD_MAGIC);
        plain.extend_from_slice(&[0; 8]);
        assert_eq!(payload_start(&plain, 4), Some(5));
    }
}
