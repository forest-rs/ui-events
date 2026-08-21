// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal `COMPOUND_TEXT` decoding for XIM strings.
//!
//! Compound text is ISO 2022 with ASCII in GL and Latin-1 in GR initially.
//! This decoder also handles UTF-8 designation (`ESC % G` ... `ESC % @`) and
//! UTF-8 extended segments, which current IM servers emit when `UTF8_STRING`
//! was not negotiated.
//! Other designated charsets (legacy CJK and similar) become U+FFFD.

use alloc::string::String;

/// The active interpretation of one half (GL or GR) of the byte range.
#[derive(Clone, Copy, PartialEq)]
enum Charset {
    /// ISO 646 IRV / ASCII (bytes `0x20`..`0x7E`).
    Ascii,
    /// ISO 8859-1 (bytes `0xA0`..`0xFF`).
    Latin1,
    /// A designated charset this decoder does not carry tables for.
    /// Each character (of the given byte width) decodes to U+FFFD.
    Unknown(u8),
}

/// Decode XIM `COMPOUND_TEXT` bytes, replacing unsupported charsets with
/// U+FFFD.
pub(super) fn decode(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut gl = Charset::Ascii;
    let mut gr = Charset::Latin1;
    let mut utf8 = false;
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == 0x1B {
            i += 1;
            i += escape(&bytes[i..], &mut gl, &mut gr, &mut utf8, &mut out);
            continue;
        }
        if utf8 {
            // Maximal run of non-escape bytes as UTF-8.
            let start = i;
            while i < bytes.len() && bytes[i] != 0x1B {
                i += 1;
            }
            out.push_str(&String::from_utf8_lossy(&bytes[start..i]));
            continue;
        }
        if byte == b'\t' || byte == b'\n' {
            out.push(char::from(byte));
            i += 1;
            continue;
        }
        let charset = if byte < 0x80 { gl } else { gr };
        match charset {
            Charset::Ascii => {
                if (0x20..0x7F).contains(&byte) {
                    out.push(char::from(byte));
                }
                i += 1;
            }
            Charset::Latin1 => {
                if byte >= 0xA0 {
                    out.push(char::from(byte));
                }
                i += 1;
            }
            Charset::Unknown(width) => {
                out.push(char::REPLACEMENT_CHARACTER);
                i += usize::from(width.max(1));
            }
        }
    }
    out
}

/// Handle the escape sequence following an `ESC` byte.
///
/// Return how many bytes of `rest` the sequence consumed.
fn escape(
    rest: &[u8],
    gl: &mut Charset,
    gr: &mut Charset,
    utf8: &mut bool,
    out: &mut String,
) -> usize {
    match rest {
        // Enter/leave UTF-8.
        [0x25, b'G', ..] => {
            *utf8 = true;
            2
        }
        [0x25, b'@', ..] => {
            *utf8 = false;
            2
        }
        // Extended segment: ESC % / F M L name STRING.
        [0x25, 0x2F, format, m, l, ..] => {
            let len = (usize::from(m & 0x7F) << 7) | usize::from(l & 0x7F);
            let segment = rest.get(5..5 + len).unwrap_or(&rest[rest.len()..]);
            extended_segment(segment, *format, out);
            5 + segment.len()
        }
        // 94-set into GL; only ASCII (final B) is decoded.
        [0x28, final_byte, ..] => {
            *gl = if *final_byte == b'B' {
                Charset::Ascii
            } else {
                Charset::Unknown(1)
            };
            2
        }
        // 94- or 96-set into GR; only Latin-1 (ESC - A) is decoded.
        [0x29, _, ..] => {
            *gr = Charset::Unknown(1);
            2
        }
        [0x2D, final_byte, ..] => {
            *gr = if *final_byte == b'A' {
                Charset::Latin1
            } else {
                Charset::Unknown(1)
            };
            2
        }
        // Multi-byte designations (ESC $ F, ESC $ ( F / ) F).
        [0x24, 0x28, _, ..] => {
            *gl = Charset::Unknown(2);
            3
        }
        [0x24, 0x29, _, ..] => {
            *gr = Charset::Unknown(2);
            3
        }
        [0x24, _, ..] => {
            *gl = Charset::Unknown(2);
            2
        }
        // Other sequences: skip intermediates (0x20..0x2F) and a final byte.
        _ => {
            let mut consumed = 0;
            while rest.get(consumed).is_some_and(|b| (0x20..0x30).contains(b)) {
                consumed += 1;
            }
            if consumed < rest.len() {
                consumed += 1;
            }
            consumed
        }
    }
}

/// Decode one extended segment: a charset name, `0x02`, then the text.
///
/// Only UTF-8 segments are decoded; anything else becomes one U+FFFD per
/// character of the format's declared width.
fn extended_segment(segment: &[u8], format: u8, out: &mut String) {
    let Some(separator) = segment.iter().position(|byte| *byte == 0x02) else {
        return;
    };
    let (name, text) = segment.split_at(separator);
    let text = &text[1..];
    if name.eq_ignore_ascii_case(b"utf-8") || name.eq_ignore_ascii_case(b"utf8") {
        out.push_str(&String::from_utf8_lossy(text));
        return;
    }
    // Format 0x31..0x34: character width one to four bytes.
    let width = usize::from(format.saturating_sub(0x30).clamp(1, 4));
    for _ in 0..text.len().div_ceil(width) {
        out.push(char::REPLACEMENT_CHARACTER);
    }
}

#[cfg(test)]
mod tests {
    use alloc::boxed::Box;

    use super::decode;

    #[test]
    fn ascii_passes_through() {
        assert_eq!(decode(b"hello, xim\n"), "hello, xim\n");
    }

    #[test]
    fn latin1_gr_bytes_decode_directly() {
        assert_eq!(decode(&[b'c', 0xE9, b't', 0xE9]), "cété");
    }

    #[test]
    fn utf8_designation_switches_the_stream() {
        let bytes: Box<[u8]> = b"a"
            .iter()
            .copied()
            .chain(b"\x1B%G".iter().copied())
            .chain("日本".as_bytes().iter().copied())
            .chain(b"\x1B%@".iter().copied())
            .chain(b"b".iter().copied())
            .collect();
        assert_eq!(decode(&bytes), "a日本b");
    }

    #[test]
    fn utf8_extended_segment_decodes() {
        let text = "語".as_bytes();
        let payload_len = "utf-8".len() + 1 + text.len();
        let len_hi = 0x80 | u8::try_from(payload_len >> 7).unwrap();
        let len_lo = 0x80 | u8::try_from(payload_len & 0x7F).unwrap();
        let bytes: Box<[u8]> = [0x1B, 0x25, 0x2F, 0x33, len_hi, len_lo]
            .into_iter()
            .chain(b"utf-8\x02".iter().copied())
            .chain(text.iter().copied())
            .collect();
        assert_eq!(decode(&bytes), "語");
    }

    #[test]
    fn unknown_multibyte_designation_becomes_replacement_characters() {
        let bytes = b"\x1B$(B\x30\x21\x30\x22";
        assert_eq!(decode(bytes), "\u{FFFD}\u{FFFD}");
    }

    #[test]
    fn unknown_gr_designation_is_replaced_until_redesignated() {
        let bytes = b"\x1B-F\xE9\x1B-A\xE9";
        assert_eq!(decode(bytes), "\u{FFFD}é");
    }

    #[test]
    fn truncated_escape_sequences_do_not_panic() {
        assert_eq!(decode(b"\x1B"), "");
        assert_eq!(decode(b"\x1B%"), "");
        assert_eq!(decode(b"\x1B%/\x31"), "");
    }
}
