// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! XIM protocol packet encoding and decoding.
//!
//! Packets are built and parsed in the byte order this client declares in
//! `XIM_CONNECT`: the machine's native order.
//! Every builder returns a complete packet including the four-byte header.
//! Every parser takes the packet body that follows the header and returns
//! `None` on malformed input.

use alloc::boxed::Box;
use alloc::vec::Vec;

// The XIM major opcodes this client sends or handles.
pub(super) const XIM_CONNECT: u8 = 1;
pub(super) const XIM_CONNECT_REPLY: u8 = 2;
pub(super) const XIM_DISCONNECT: u8 = 3;
pub(super) const XIM_ERROR: u8 = 20;
pub(super) const XIM_OPEN: u8 = 30;
pub(super) const XIM_OPEN_REPLY: u8 = 31;
pub(super) const XIM_CLOSE: u8 = 32;
pub(super) const XIM_REGISTER_TRIGGERKEYS: u8 = 34;
pub(super) const XIM_SET_EVENT_MASK: u8 = 37;
pub(super) const XIM_ENCODING_NEGOTIATION: u8 = 38;
pub(super) const XIM_ENCODING_NEGOTIATION_REPLY: u8 = 39;
pub(super) const XIM_GET_IM_VALUES: u8 = 44;
pub(super) const XIM_GET_IM_VALUES_REPLY: u8 = 45;
pub(super) const XIM_CREATE_IC: u8 = 50;
pub(super) const XIM_CREATE_IC_REPLY: u8 = 51;
pub(super) const XIM_DESTROY_IC: u8 = 52;
pub(super) const XIM_SET_IC_VALUES: u8 = 54;
pub(super) const XIM_SET_IC_VALUES_REPLY: u8 = 55;
pub(super) const XIM_SET_IC_FOCUS: u8 = 58;
pub(super) const XIM_UNSET_IC_FOCUS: u8 = 59;
pub(super) const XIM_FORWARD_EVENT: u8 = 60;
pub(super) const XIM_SYNC: u8 = 61;
pub(super) const XIM_SYNC_REPLY: u8 = 62;
pub(super) const XIM_COMMIT: u8 = 63;
pub(super) const XIM_PREEDIT_START: u8 = 73;
pub(super) const XIM_PREEDIT_START_REPLY: u8 = 74;
pub(super) const XIM_PREEDIT_DRAW: u8 = 75;
pub(super) const XIM_PREEDIT_CARET: u8 = 76;
pub(super) const XIM_PREEDIT_CARET_REPLY: u8 = 77;
pub(super) const XIM_PREEDIT_DONE: u8 = 78;

/// `XIM_FORWARD_EVENT` / `XIM_COMMIT` flag bit: the receiver must answer with
/// `XIM_SYNC_REPLY` once the message has been processed.
pub(super) const FLAG_SYNCHRONOUS: u16 = 0x0001;

/// `XIM_COMMIT` flag bit: the packet carries a committed string.
pub(super) const COMMIT_CHARS: u16 = 0x0002;
/// `XIM_COMMIT` flag bit: the packet carries a keysym.
pub(super) const COMMIT_KEYSYM: u16 = 0x0004;

/// `XIM_PREEDIT_DRAW` status bit: the draw carries no text (pure deletion).
pub(super) const DRAW_NO_STRING: u32 = 0x0000_0001;

/// `XIM_PREEDIT_CARET` direction for an absolute caret position.
pub(super) const CARET_ABSOLUTE_POSITION: u32 = 10;

/// Bytes needed to round `n` up to a multiple of four.
pub(super) const fn pad4(n: usize) -> usize {
    (4 - (n % 4)) % 4
}

/// Incremental packet writer; [`finish`](Self::finish) patches the header.
struct Writer {
    /// Packet bytes so far: 4-byte header then body, native endian.
    buf: Vec<u8>,
}

impl Writer {
    fn new(major: u8) -> Self {
        Self {
            buf: alloc::vec![major, 0, 0, 0],
        }
    }

    fn u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.buf.extend_from_slice(&value.to_ne_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_ne_bytes());
    }

    fn i32(&mut self, value: i32) {
        self.buf.extend_from_slice(&value.to_ne_bytes());
    }

    fn bytes(&mut self, value: &[u8]) {
        self.buf.extend_from_slice(value);
    }

    fn zeros(&mut self, count: usize) {
        self.buf.resize(self.buf.len() + count, 0);
    }

    fn finish(mut self) -> Box<[u8]> {
        debug_assert!(self.buf.len() % 4 == 0, "XIM packets are padded to 4 bytes");
        let units = u16::try_from((self.buf.len() - 4) / 4).unwrap_or(u16::MAX);
        self.buf[2..4].copy_from_slice(&units.to_ne_bytes());
        self.buf.into_boxed_slice()
    }
}

/// Checked packet-body reader in the declared byte order.
struct Reader<'a> {
    /// Packet body bytes (no XIM header).
    data: &'a [u8],
    /// Next byte index to read.
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn u16(&mut self) -> Option<u16> {
        let bytes = self.bytes(2)?;
        Some(u16::from_ne_bytes([bytes[0], bytes[1]]))
    }

    fn i16(&mut self) -> Option<i16> {
        let bytes = self.bytes(2)?;
        Some(i16::from_ne_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        let bytes = self.bytes(4)?;
        Some(u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn i32(&mut self) -> Option<i32> {
        self.u32().map(|value| value as i32)
    }

    fn bytes(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(count)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        self.bytes(count).map(|_| ())
    }
}

/// Split one packet off the front of `buf`.
///
/// Returns the major opcode, the packet body, and the total encoded size.
/// Returns `None` when `buf` has no complete packet, including trailing
/// zero padding (identified by a zero major opcode).
pub(super) fn split_packet(buf: &[u8]) -> Option<(u8, &[u8], usize)> {
    let header = buf.get(..4)?;
    let major = header[0];
    if major == 0 {
        return None;
    }
    let units = u16::from_ne_bytes([header[2], header[3]]);
    let total = 4 + usize::from(units) * 4;
    let body = buf.get(4..total)?;
    Some((major, body, total))
}

/// Build `XIM_CONNECT`: native byte order, protocol 1.0, no authentication.
pub(super) fn connect_packet() -> Box<[u8]> {
    let mut w = Writer::new(XIM_CONNECT);
    w.u8(if cfg!(target_endian = "little") {
        0x6C // 'l'
    } else {
        0x42 // 'B'
    });
    w.u8(0);
    w.u16(1);
    w.u16(0);
    w.u16(0);
    w.finish()
}

/// Build `XIM_OPEN` carrying the locale name.
pub(super) fn open_packet(locale: &str) -> Box<[u8]> {
    let name = locale.as_bytes();
    let len = name.len().min(255);
    let mut w = Writer::new(XIM_OPEN);
    #[expect(clippy::cast_possible_truncation, reason = "len is clamped to 255")]
    w.u8(len as u8);
    w.bytes(&name[..len]);
    w.zeros(pad4(1 + len));
    w.finish()
}

/// Encodings offered in negotiation, in preference order.
///
/// `UTF8_STRING` first (fcitx 5 and similar send UTF-8).
/// `COMPOUND_TEXT` is the mandatory baseline.
const ENCODINGS: [&[u8]; 2] = [b"UTF8_STRING", b"COMPOUND_TEXT"];

/// The negotiated-encoding index corresponding to `UTF8_STRING`.
pub(super) const ENCODING_INDEX_UTF8: i16 = 0;

/// Build `XIM_ENCODING_NEGOTIATION` offering [`ENCODINGS`] by name.
pub(super) fn encoding_negotiation_packet(im: u16) -> Box<[u8]> {
    let names_len: usize = ENCODINGS.iter().map(|name| 1 + name.len()).sum();
    let mut w = Writer::new(XIM_ENCODING_NEGOTIATION);
    w.u16(im);
    w.u16(u16::try_from(names_len).unwrap_or(0));
    for name in ENCODINGS {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "encoding names are short constants"
        )]
        w.u8(name.len() as u8);
        w.bytes(name);
    }
    w.zeros(pad4(names_len));
    w.u16(0); // no encodings listed by detailed data
    w.u16(0);
    w.finish()
}

/// Build `XIM_GET_IM_VALUES` querying a single IM attribute.
pub(super) fn get_im_values_packet(im: u16, attr: u16) -> Box<[u8]> {
    let mut w = Writer::new(XIM_GET_IM_VALUES);
    w.u16(im);
    w.u16(2);
    w.u16(attr);
    w.zeros(pad4(2));
    w.finish()
}

/// Serialize a list of `XICATTRIBUTE` entries.
fn encode_attributes(w: &mut Writer, attrs: &[(u16, Box<[u8]>)]) {
    for (id, value) in attrs {
        w.u16(*id);
        w.u16(u16::try_from(value.len()).unwrap_or(0));
        w.bytes(value);
        w.zeros(pad4(value.len()));
    }
}

/// Byte length of a list of `XICATTRIBUTE` entries.
fn attributes_len(attrs: &[(u16, Box<[u8]>)]) -> usize {
    attrs
        .iter()
        .map(|(_, value)| 4 + value.len() + pad4(value.len()))
        .sum()
}

/// Build `XIM_CREATE_IC` with the given IC attributes.
pub(super) fn create_ic_packet(im: u16, attrs: &[(u16, Box<[u8]>)]) -> Box<[u8]> {
    let mut w = Writer::new(XIM_CREATE_IC);
    w.u16(im);
    w.u16(u16::try_from(attributes_len(attrs)).unwrap_or(0));
    encode_attributes(&mut w, attrs);
    w.finish()
}

/// Build `XIM_SET_IC_VALUES` with the given IC attributes.
pub(super) fn set_ic_values_packet(im: u16, ic: u16, attrs: &[(u16, Box<[u8]>)]) -> Box<[u8]> {
    let mut w = Writer::new(XIM_SET_IC_VALUES);
    w.u16(im);
    w.u16(ic);
    w.u16(u16::try_from(attributes_len(attrs)).unwrap_or(0));
    w.u16(0);
    encode_attributes(&mut w, attrs);
    w.finish()
}

/// Packet whose body is only the IM and IC ids.
///
/// Used for `XIM_SET_IC_FOCUS`, `XIM_UNSET_IC_FOCUS`, `XIM_DESTROY_IC`, and
/// `XIM_SYNC_REPLY`.
pub(super) fn ids_packet(major: u8, im: u16, ic: u16) -> Box<[u8]> {
    let mut w = Writer::new(major);
    w.u16(im);
    w.u16(ic);
    w.finish()
}

/// Build `XIM_CLOSE`.
pub(super) fn close_packet(im: u16) -> Box<[u8]> {
    let mut w = Writer::new(XIM_CLOSE);
    w.u16(im);
    w.u16(0);
    w.finish()
}

/// Build `XIM_DISCONNECT`.
pub(super) fn disconnect_packet() -> Box<[u8]> {
    Writer::new(XIM_DISCONNECT).finish()
}

/// Build `XIM_FORWARD_EVENT` around a serialized core protocol event.
pub(super) fn forward_event_packet(im: u16, ic: u16, flags: u16, event: &[u8; 32]) -> Box<[u8]> {
    let mut w = Writer::new(XIM_FORWARD_EVENT);
    w.u16(im);
    w.u16(ic);
    w.u16(flags);
    w.u16(0); // upper 16 bits of the event's serial number
    w.bytes(event);
    w.finish()
}

/// Build `XIM_PREEDIT_START_REPLY`.
///
/// A negative return value means the preedit string length is unlimited.
pub(super) fn preedit_start_reply_packet(im: u16, ic: u16, return_value: i32) -> Box<[u8]> {
    let mut w = Writer::new(XIM_PREEDIT_START_REPLY);
    w.u16(im);
    w.u16(ic);
    w.i32(return_value);
    w.finish()
}

/// Build `XIM_PREEDIT_CARET_REPLY` echoing the resulting caret position.
pub(super) fn preedit_caret_reply_packet(im: u16, ic: u16, position: u32) -> Box<[u8]> {
    let mut w = Writer::new(XIM_PREEDIT_CARET_REPLY);
    w.u16(im);
    w.u16(ic);
    w.u32(position);
    w.finish()
}

/// Encode a `CARD32` attribute value.
pub(super) fn u32_value(value: u32) -> Box<[u8]> {
    Box::from(value.to_ne_bytes())
}

/// Encode an `XPoint` attribute value.
pub(super) fn point_value(x: i16, y: i16) -> Box<[u8]> {
    let mut value = [0_u8; 4];
    value[0..2].copy_from_slice(&x.to_ne_bytes());
    value[2..4].copy_from_slice(&y.to_ne_bytes());
    Box::from(value)
}

/// Encode an `XRectangle` attribute value.
pub(super) fn rectangle_value(x: i16, y: i16, width: u16, height: u16) -> Box<[u8]> {
    let mut value = [0_u8; 8];
    value[0..2].copy_from_slice(&x.to_ne_bytes());
    value[2..4].copy_from_slice(&y.to_ne_bytes());
    value[4..6].copy_from_slice(&width.to_ne_bytes());
    value[6..8].copy_from_slice(&height.to_ne_bytes());
    Box::from(value)
}

/// Encode a nested-list attribute value from inner `XICATTRIBUTE` entries.
pub(super) fn nested_value(attrs: &[(u16, Box<[u8]>)]) -> Box<[u8]> {
    let mut w = Writer {
        buf: Vec::with_capacity(attributes_len(attrs)),
    };
    encode_attributes(&mut w, attrs);
    w.buf.into_boxed_slice()
}

/// One attribute definition from the `XIM_OPEN_REPLY` dictionaries.
pub(super) struct AttrDefinition {
    /// Attribute id used in later `XIM_GET_IM_VALUES` / `XIM_CREATE_IC` / etc.
    pub(super) id: u16,
    /// Attribute name bytes (for example `b"inputStyle"`), without a NUL.
    pub(super) name: Box<[u8]>,
}

/// The decoded `XIM_OPEN_REPLY`.
pub(super) struct OpenReply {
    /// Input-method id assigned by the server.
    pub(super) im_id: u16,
    /// IM-level attribute dictionary (`queryInputStyle`, ...).
    pub(super) im_attrs: Box<[AttrDefinition]>,
    /// IC-level attribute dictionary (`inputStyle`, `clientWindow`, ...).
    pub(super) ic_attrs: Box<[AttrDefinition]>,
}

/// Parse one `XIMATTR`/`XICATTR` dictionary list of `len` bytes.
fn parse_attr_definitions(r: &mut Reader<'_>, len: usize) -> Option<Box<[AttrDefinition]>> {
    let mut attrs = Vec::new();
    let end = r.pos.checked_add(len)?;
    while r.pos < end {
        let id = r.u16()?;
        let _type = r.u16()?;
        let name_len = usize::from(r.u16()?);
        let name = Box::<[u8]>::from(r.bytes(name_len)?);
        r.skip(pad4(2 + name_len))?;
        attrs.push(AttrDefinition { id, name });
    }
    (r.pos == end).then_some(attrs.into_boxed_slice())
}

/// Parse `XIM_OPEN_REPLY`.
pub(super) fn parse_open_reply(body: &[u8]) -> Option<OpenReply> {
    let mut r = Reader::new(body);
    let im_id = r.u16()?;
    let im_len = usize::from(r.u16()?);
    let im_attrs = parse_attr_definitions(&mut r, im_len)?;
    let ic_len = usize::from(r.u16()?);
    r.skip(2)?;
    let ic_attrs = parse_attr_definitions(&mut r, ic_len)?;
    Some(OpenReply {
        im_id,
        im_attrs,
        ic_attrs,
    })
}

/// Parse `XIM_ENCODING_NEGOTIATION_REPLY` into (category, index).
pub(super) fn parse_encoding_reply(body: &[u8]) -> Option<(u16, i16)> {
    let mut r = Reader::new(body);
    r.skip(2)?; // input-method-ID
    let category = r.u16()?;
    let index = r.i16()?;
    Some((category, index))
}

/// Parse the `XIMStyles` value out of `XIM_GET_IM_VALUES_REPLY` for the
/// queried attribute id.
pub(super) fn parse_styles_reply(body: &[u8], style_attr: u16) -> Option<Box<[u32]>> {
    let mut r = Reader::new(body);
    r.skip(2)?; // input-method-ID
    let list_len = usize::from(r.u16()?);
    let end = r.pos.checked_add(list_len)?;
    while r.pos < end {
        let id = r.u16()?;
        let value_len = usize::from(r.u16()?);
        let value = r.bytes(value_len)?;
        r.skip(pad4(value_len))?;
        if id != style_attr {
            continue;
        }
        let mut v = Reader::new(value);
        let count = usize::from(v.u16()?);
        v.skip(2)?;
        let mut styles = Vec::with_capacity(count);
        for _ in 0..count {
            styles.push(v.u32()?);
        }
        return Some(styles.into_boxed_slice());
    }
    None
}

/// Parse `XIM_CREATE_IC_REPLY` into (im, ic).
pub(super) fn parse_create_ic_reply(body: &[u8]) -> Option<(u16, u16)> {
    let mut r = Reader::new(body);
    Some((r.u16()?, r.u16()?))
}

/// Parse `XIM_SET_EVENT_MASK` into (im, ic, forward mask, synchronous mask).
pub(super) fn parse_set_event_mask(body: &[u8]) -> Option<(u16, u16, u32, u32)> {
    let mut r = Reader::new(body);
    Some((r.u16()?, r.u16()?, r.u32()?, r.u32()?))
}

/// Parse `XIM_FORWARD_EVENT` into (im, ic, flags, core event bytes).
pub(super) fn parse_forward_event(body: &[u8]) -> Option<(u16, u16, u16, [u8; 32])> {
    let mut r = Reader::new(body);
    let im = r.u16()?;
    let ic = r.u16()?;
    let flags = r.u16()?;
    r.skip(2)?; // upper 16 bits of the serial number
    let event: [u8; 32] = r.bytes(32)?.try_into().ok()?;
    Some((im, ic, flags, event))
}

/// Parse `XIM_COMMIT` into (im, ic, flags, committed string bytes).
///
/// A keysym-only commit yields empty string bytes.
pub(super) fn parse_commit(body: &[u8]) -> Option<(u16, u16, u16, Box<[u8]>)> {
    let mut r = Reader::new(body);
    let im = r.u16()?;
    let ic = r.u16()?;
    let flags = r.u16()?;
    if flags & COMMIT_KEYSYM != 0 {
        r.skip(2)?; // unused
        r.skip(4)?; // keysym
    }
    let text = if flags & COMMIT_CHARS != 0 {
        let len = usize::from(r.u16()?);
        Box::<[u8]>::from(r.bytes(len)?)
    } else {
        Box::from([])
    };
    Some((im, ic, flags, text))
}

/// The decoded body of `XIM_PREEDIT_DRAW`.
pub(super) struct PreeditDraw {
    /// Caret position after the draw, in characters.
    pub(super) caret: i32,
    /// First character index of the range to replace.
    pub(super) chg_first: i32,
    /// Length of the range to replace, in characters.
    pub(super) chg_length: i32,
    /// Status bits; bit 0 ([`DRAW_NO_STRING`]) means the text field is empty.
    pub(super) status: u32,
    /// Replacement text in the negotiated wire encoding.
    /// Empty when [`DRAW_NO_STRING`] is set.
    pub(super) text: Box<[u8]>,
}

/// Parse `XIM_PREEDIT_DRAW`.
///
/// The feedback styling array is not used.
pub(super) fn parse_preedit_draw(body: &[u8]) -> Option<(u16, u16, PreeditDraw)> {
    let mut r = Reader::new(body);
    let im = r.u16()?;
    let ic = r.u16()?;
    let caret = r.i32()?;
    let chg_first = r.i32()?;
    let chg_length = r.i32()?;
    let status = r.u32()?;
    let text_len = usize::from(r.u16()?);
    let text = Box::<[u8]>::from(r.bytes(text_len)?);
    Some((
        im,
        ic,
        PreeditDraw {
            caret,
            chg_first,
            chg_length,
            status,
            text,
        },
    ))
}

/// Parse `XIM_PREEDIT_CARET` into (im, ic, position, direction).
pub(super) fn parse_preedit_caret(body: &[u8]) -> Option<(u16, u16, i32, u32)> {
    let mut r = Reader::new(body);
    Some((r.u16()?, r.u16()?, r.i32()?, r.u32()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attr_definition(id: u16, name: &[u8]) -> Box<[u8]> {
        let pad = [0_u8; 3];
        let pad_n = pad4(2 + name.len());
        id.to_ne_bytes()
            .into_iter()
            .chain(0_u16.to_ne_bytes())
            .chain(u16::try_from(name.len()).unwrap().to_ne_bytes())
            .chain(name.iter().copied())
            .chain(pad[..pad_n].iter().copied())
            .collect()
    }

    #[test]
    fn packets_are_padded_and_carry_the_length_in_units() {
        let packet = open_packet("en_US.UTF-8");
        assert_eq!(packet[0], XIM_OPEN);
        assert_eq!(packet.len() % 4, 0);
        let units = u16::from_ne_bytes([packet[2], packet[3]]);
        assert_eq!(usize::from(units) * 4 + 4, packet.len());
        assert_eq!(packet[4], 11);
        assert_eq!(&packet[5..16], b"en_US.UTF-8");
    }

    #[test]
    fn connect_packet_declares_native_byte_order() {
        let packet = connect_packet();
        let expected = if cfg!(target_endian = "little") {
            0x6C
        } else {
            0x42
        };
        assert_eq!(packet[4], expected);
        let expected: Box<[u8]> = 1_u16
            .to_ne_bytes()
            .into_iter()
            .chain(0_u16.to_ne_bytes())
            .chain(0_u16.to_ne_bytes())
            .collect();
        assert_eq!(&packet[6..12], &*expected);
    }

    #[test]
    fn split_packet_consumes_one_packet_and_stops_at_padding() {
        let first = ids_packet(XIM_SYNC_REPLY, 7, 9);
        let second = ids_packet(XIM_SET_IC_FOCUS, 7, 9);
        let buf: Box<[u8]> = first
            .iter()
            .copied()
            .chain(second.iter().copied())
            .chain([0_u8; 8])
            .collect();

        let (major, body, consumed) = split_packet(&buf).expect("first packet");
        assert_eq!(major, XIM_SYNC_REPLY);
        assert_eq!(consumed, first.len());
        assert_eq!(body.len(), 4);

        let (major, _, consumed_2) = split_packet(&buf[consumed..]).expect("second packet");
        assert_eq!(major, XIM_SET_IC_FOCUS);
        assert!(split_packet(&buf[consumed + consumed_2..]).is_none());
    }

    #[test]
    fn split_packet_returns_none_for_incomplete_input() {
        let packet = ids_packet(XIM_SYNC_REPLY, 1, 2);
        assert!(split_packet(&packet[..3]).is_none());
        assert!(split_packet(&packet[..packet.len() - 1]).is_none());
    }

    #[test]
    fn open_reply_yields_both_attribute_dictionaries() {
        let im_list = attr_definition(10, b"queryInputStyle");
        let ic0 = attr_definition(1, b"inputStyle");
        let ic1 = attr_definition(2, b"clientWindow");
        let ic_list: Box<[u8]> = ic0.iter().copied().chain(ic1.iter().copied()).collect();
        let body: Box<[u8]> = 3_u16
            .to_ne_bytes()
            .into_iter()
            .chain(u16::try_from(im_list.len()).unwrap().to_ne_bytes())
            .chain(im_list.iter().copied())
            .chain(u16::try_from(ic_list.len()).unwrap().to_ne_bytes())
            .chain(0_u16.to_ne_bytes())
            .chain(ic_list.iter().copied())
            .collect();

        let reply = parse_open_reply(&body).expect("well-formed reply");
        assert_eq!(reply.im_id, 3);
        assert_eq!(reply.im_attrs.len(), 1);
        assert_eq!(reply.im_attrs[0].id, 10);
        assert_eq!(&*reply.im_attrs[0].name, b"queryInputStyle");
        assert_eq!(reply.ic_attrs.len(), 2);
        assert_eq!(&*reply.ic_attrs[1].name, b"clientWindow");
    }

    #[test]
    fn styles_reply_yields_the_advertised_styles() {
        let value: Box<[u8]> = 2_u16
            .to_ne_bytes()
            .into_iter()
            .chain(0_u16.to_ne_bytes())
            .chain(0x0402_u32.to_ne_bytes())
            .chain(0x0408_u32.to_ne_bytes())
            .collect();
        let body: Box<[u8]> = 3_u16
            .to_ne_bytes()
            .into_iter()
            .chain(u16::try_from(4 + value.len()).unwrap().to_ne_bytes())
            .chain(10_u16.to_ne_bytes())
            .chain(u16::try_from(value.len()).unwrap().to_ne_bytes())
            .chain(value.iter().copied())
            .collect();

        assert_eq!(
            &*parse_styles_reply(&body, 10).expect("styles present"),
            &[0x0402_u32, 0x0408]
        );
        assert!(parse_styles_reply(&body, 11).is_none());
    }

    #[test]
    fn commit_parses_chars_keysym_and_both_layouts() {
        let body: Box<[u8]> = 1_u16
            .to_ne_bytes()
            .into_iter()
            .chain(2_u16.to_ne_bytes())
            .chain((FLAG_SYNCHRONOUS | COMMIT_CHARS).to_ne_bytes())
            .chain(2_u16.to_ne_bytes())
            .chain(*b"hi")
            .chain([0; 2])
            .collect();
        let (_, _, flags, text) = parse_commit(&body).expect("chars commit");
        assert_eq!(flags & COMMIT_CHARS, COMMIT_CHARS);
        assert_eq!(&*text, b"hi");

        let body: Box<[u8]> = 1_u16
            .to_ne_bytes()
            .into_iter()
            .chain(2_u16.to_ne_bytes())
            .chain((COMMIT_CHARS | COMMIT_KEYSYM).to_ne_bytes())
            .chain(0_u16.to_ne_bytes())
            .chain(0x61_u32.to_ne_bytes())
            .chain(1_u16.to_ne_bytes())
            .chain(*b"a")
            .chain([0; 1])
            .collect();
        let (_, _, _, text) = parse_commit(&body).expect("both commit");
        assert_eq!(&*text, b"a");

        let body: Box<[u8]> = 1_u16
            .to_ne_bytes()
            .into_iter()
            .chain(2_u16.to_ne_bytes())
            .chain(COMMIT_KEYSYM.to_ne_bytes())
            .chain(0_u16.to_ne_bytes())
            .chain(0x61_u32.to_ne_bytes())
            .collect();
        let (_, _, _, text) = parse_commit(&body).expect("keysym commit");
        assert!(text.is_empty());
    }

    #[test]
    fn preedit_draw_parses_change_range_and_text() {
        let body: Box<[u8]> = 1_u16
            .to_ne_bytes()
            .into_iter()
            .chain(2_u16.to_ne_bytes())
            .chain(3_i32.to_ne_bytes())
            .chain(0_i32.to_ne_bytes())
            .chain(2_i32.to_ne_bytes())
            .chain(0_u32.to_ne_bytes())
            .chain(3_u16.to_ne_bytes())
            .chain(*b"abc")
            .chain([0; 3]) // Pad(2 + 3)
            .chain(0_u16.to_ne_bytes())
            .chain(0_u16.to_ne_bytes())
            .collect();

        let (_, _, draw) = parse_preedit_draw(&body).expect("draw");
        assert_eq!(draw.caret, 3);
        assert_eq!(draw.chg_first, 0);
        assert_eq!(draw.chg_length, 2);
        assert_eq!(&*draw.text, b"abc");
    }

    #[test]
    fn ic_attribute_encoding_pads_values_to_four_bytes() {
        let attrs = [(5_u16, point_value(10, -20))];
        let packet = set_ic_values_packet(1, 2, &attrs);
        assert_eq!(packet.len(), 4 + 4 + 4 + 8);
        let nested = nested_value(&attrs);
        assert_eq!(nested.len(), 8);
        assert_eq!(&nested[..2], &5_u16.to_ne_bytes());
        assert_eq!(&nested[2..4], &4_u16.to_ne_bytes());
    }

    #[test]
    fn forward_event_wraps_the_serialized_core_event() {
        let event = [7_u8; 32];
        let packet = forward_event_packet(3, 4, FLAG_SYNCHRONOUS, &event);
        assert_eq!(packet.len(), 4 + 8 + 32);
        assert_eq!(&packet[12..], &event);
    }
}
