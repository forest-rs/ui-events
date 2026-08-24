// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Platform-neutral values for the Wayland text-input-v3 protocol.
//!
//! This module deliberately stops short of owning a `zwp_text_input_v3` proxy.
//! Focus, commit serials, and protocol double-buffering belong beside the
//! Wayland event queue. The helpers here validate and translate the values sent
//! across that stateful boundary.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use ui_events::text::{CompositionState, TextInputEvent, TextRange, TextRangeEncoding};
use ui_text_input::{
    SurroundingTextProvider, SurroundingTextRequest, TextInputConfiguration, TextInputHints,
    TextInputHost, TextInputPurpose, TextInputRect, TextRangeConverter,
};

/// Maximum UTF-8 byte length accepted by text-input-v3 surrounding text.
pub const MAX_SURROUNDING_TEXT_BYTES: u32 = 4000;

/// Surrounding text ready for `zwp_text_input_v3.set_surrounding_text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaylandSurroundingText {
    revision: u64,
    text: String,
    cursor: i32,
    anchor: i32,
}

impl WaylandSurroundingText {
    /// Return the coherent host revision used to build this value.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Return surrounding UTF-8 text with any active preedit omitted.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Return the active cursor as a byte offset within [`Self::text`].
    pub const fn cursor(&self) -> i32 {
        self.cursor
    }

    /// Return the selection anchor as a byte offset within [`Self::text`].
    pub const fn anchor(&self) -> i32 {
        self.anchor
    }
}

/// Query bounded surrounding text and adapt it to text-input-v3.
///
/// Wayland requires UTF-8, the complete selection, at most 4,000 bytes, and
/// omission of any active preedit text. The host snapshot and surrounding text
/// must carry the same revision so composition removal cannot combine state
/// from different editor versions.
pub fn surrounding_text_from_host(
    host: &(impl TextInputHost + SurroundingTextProvider + TextRangeConverter + ?Sized),
    before_length: u32,
    after_length: u32,
) -> Option<WaylandSurroundingText> {
    let state = host.text_input_snapshot();
    let surrounding = host.surrounding_text(SurroundingTextRequest::new(
        before_length,
        after_length,
        TextRangeEncoding::Utf8Bytes,
        MAX_SURROUNDING_TEXT_BYTES,
    ))?;
    if state.revision() != surrounding.revision() {
        return None;
    }

    let source = surrounding.text();
    let source_range = source.range();
    if source_range.encoding != TextRangeEncoding::Utf8Bytes {
        return None;
    }
    let mut text = source.text().to_string();
    let mut cursor = surrounding.relative_active();
    let mut anchor = surrounding.relative_anchor();

    if let Some(composition) = state.composition() {
        let composition = if composition.range().encoding == TextRangeEncoding::Utf8Bytes {
            composition.range()
        } else {
            host.convert_range(composition.range(), TextRangeEncoding::Utf8Bytes)?
        };
        let start = composition
            .range
            .start
            .checked_sub(source_range.range.start)?;
        let end = composition
            .range
            .end
            .checked_sub(source_range.range.start)?;
        if composition.range.end > source_range.range.end {
            return None;
        }
        let omitted =
            ui_events::text::TextTargetRange::utf8_bytes(start, end).to_range_in(&text)?;
        text.replace_range(omitted, "");
        cursor = offset_after_omission(cursor, start, end)?;
        anchor = offset_after_omission(anchor, start, end)?;
    }

    if text.len() > MAX_SURROUNDING_TEXT_BYTES as usize {
        return None;
    }
    Some(WaylandSurroundingText {
        revision: surrounding.revision(),
        text,
        cursor: i32::try_from(cursor).ok()?,
        anchor: i32::try_from(anchor).ok()?,
    })
}

fn offset_after_omission(offset: u32, omitted_start: u32, omitted_end: u32) -> Option<u32> {
    if offset <= omitted_start {
        Some(offset)
    } else if offset < omitted_end {
        Some(omitted_start)
    } else {
        offset.checked_sub(omitted_end.checked_sub(omitted_start)?)
    }
}

/// Raw text-input-v3 content hint and purpose values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WaylandContentType {
    /// Bitset for `zwp_text_input_v3.content_hint`.
    pub hint: u32,
    /// Value for `zwp_text_input_v3.content_purpose`.
    pub purpose: u32,
}

/// Convert portable text configuration to text-input-v3 content values.
pub const fn content_type_from_configuration(
    configuration: TextInputConfiguration,
) -> WaylandContentType {
    let hints = configuration.hints;
    let mut hint = 0;
    if hints.contains(TextInputHints::COMPLETION) {
        hint |= 0x001;
    }
    if hints.contains(TextInputHints::SPELLCHECK) {
        hint |= 0x002;
    }
    if hints.contains(TextInputHints::AUTO_CAPITALIZATION) {
        hint |= 0x004;
    }
    if hints.contains(TextInputHints::LOWERCASE) {
        hint |= 0x008;
    }
    if hints.contains(TextInputHints::UPPERCASE) {
        hint |= 0x010;
    }
    if hints.contains(TextInputHints::TITLECASE) {
        hint |= 0x020;
    }
    if hints.contains(TextInputHints::HIDDEN_TEXT) {
        hint |= 0x040;
    }
    if hints.contains(TextInputHints::SENSITIVE_DATA) {
        hint |= 0x080;
    }
    if hints.contains(TextInputHints::LATIN) {
        hint |= 0x100;
    }
    if hints.contains(TextInputHints::MULTILINE) {
        hint |= 0x200;
    }

    let purpose = match configuration.purpose {
        TextInputPurpose::Normal => 0,
        TextInputPurpose::Alpha => 1,
        TextInputPurpose::Digits => 2,
        TextInputPurpose::Number => 3,
        TextInputPurpose::Phone => 4,
        TextInputPurpose::Url => 5,
        TextInputPurpose::Email => 6,
        TextInputPurpose::Name => 7,
        TextInputPurpose::Password => 8,
        TextInputPurpose::Pin => 9,
        TextInputPurpose::Date => 10,
        TextInputPurpose::Time => 11,
        TextInputPurpose::DateTime => 12,
        TextInputPurpose::Terminal => 13,
        _ => 0,
    };
    WaylandContentType { hint, purpose }
}

/// Surface-local integer cursor rectangle for text-input-v3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WaylandCursorRectangle {
    /// Surface-local X coordinate.
    pub x: i32,
    /// Surface-local Y coordinate.
    pub y: i32,
    /// Rectangle width in surface-local units.
    pub width: i32,
    /// Rectangle height in surface-local units.
    pub height: i32,
}

/// Convert a physical-pixel text rectangle to a covering Wayland rectangle.
///
/// The origin is rounded down and the far edge up so fractional scaling never
/// shrinks the caret or character bounds advertised to the compositor.
pub fn cursor_rectangle_from_physical(
    rect: TextInputRect,
    scale_factor: f64,
) -> Option<WaylandCursorRectangle> {
    if !scale_factor.is_finite()
        || scale_factor <= 0.0
        || !rect.origin.x.is_finite()
        || !rect.origin.y.is_finite()
        || !rect.size.width.is_finite()
        || !rect.size.height.is_finite()
        || rect.size.width < 0.0
        || rect.size.height < 0.0
    {
        return None;
    }
    let left = floor_i32(rect.origin.x / scale_factor)?;
    let top = floor_i32(rect.origin.y / scale_factor)?;
    let right = ceil_i32((rect.origin.x + rect.size.width) / scale_factor)?;
    let bottom = ceil_i32((rect.origin.y + rect.size.height) / scale_factor)?;
    Some(WaylandCursorRectangle {
        x: left,
        y: top,
        width: right.checked_sub(left)?,
        height: bottom.checked_sub(top)?,
    })
}

fn floor_i32(value: f64) -> Option<i32> {
    checked_i32(floor(value))
}

fn ceil_i32(value: f64) -> Option<i32> {
    checked_i32(ceil(value))
}

#[cfg(feature = "std")]
fn floor(value: f64) -> f64 {
    value.floor()
}

#[cfg(all(not(feature = "std"), feature = "libm"))]
fn floor(value: f64) -> f64 {
    libm::floor(value)
}

#[cfg(feature = "std")]
fn ceil(value: f64) -> f64 {
    value.ceil()
}

#[cfg(all(not(feature = "std"), feature = "libm"))]
fn ceil(value: f64) -> f64 {
    libm::ceil(value)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "range checks make the f64-to-i32 conversion exact for integral values"
)]
fn checked_i32(value: f64) -> Option<i32> {
    (value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX)).then_some(value as i32)
}

/// Error validating a pending text-input-v3 preedit event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WaylandPreeditError {
    /// Only one of the two cursor values used the protocol's hidden `-1` value.
    PartiallyHiddenCursor,
    /// A cursor value was negative but not the hidden `-1` pair.
    NegativeCursor,
    /// The cursor range was out of bounds or split a UTF-8 code point.
    InvalidCursorRange,
}

/// Pending text-input-v3 events collected until the compositor's `done` event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WaylandTextInputBatch {
    preedit: Option<Option<CompositionState>>,
    commit: Option<Option<String>>,
    delete: Option<(u32, u32)>,
}

impl WaylandTextInputBatch {
    /// Record a pending `preedit_string` event.
    ///
    /// The `(-1, -1)` cursor pair hides the preedit cursor. Other pairs are
    /// normalized to an ordered UTF-8 selection and validated against `text`.
    pub fn set_preedit(
        &mut self,
        text: Option<&str>,
        cursor_begin: i32,
        cursor_end: i32,
    ) -> Result<(), WaylandPreeditError> {
        let Some(text) = text else {
            self.preedit = Some(None);
            return Ok(());
        };
        let mut state = CompositionState::new(text);
        match (cursor_begin, cursor_end) {
            (-1, -1) => {}
            (-1, _) | (_, -1) => return Err(WaylandPreeditError::PartiallyHiddenCursor),
            (begin, end) if begin < 0 || end < 0 => {
                return Err(WaylandPreeditError::NegativeCursor);
            }
            (begin, end) => {
                let begin =
                    u32::try_from(begin).map_err(|_| WaylandPreeditError::InvalidCursorRange)?;
                let end =
                    u32::try_from(end).map_err(|_| WaylandPreeditError::InvalidCursorRange)?;
                let selection = TextRange::new(begin.min(end), begin.max(end));
                state = state
                    .try_with_selection(selection)
                    .ok_or(WaylandPreeditError::InvalidCursorRange)?;
            }
        }
        self.preedit = Some(Some(state));
        Ok(())
    }

    /// Record a pending `commit_string` event.
    pub fn set_commit_string(&mut self, text: Option<&str>) {
        self.commit = Some(text.map(ToString::to_string));
    }

    /// Record a pending `delete_surrounding_text` event.
    pub const fn set_delete_surrounding_text(&mut self, before_length: u32, after_length: u32) {
        self.delete = Some((before_length, after_length));
    }

    /// Consume the pending values in text-input-v3's required `done` order.
    ///
    /// `had_preedit` describes the editor state before this batch. The returned
    /// events must be applied atomically by the caller before it refreshes and
    /// commits surrounding text.
    pub fn into_events(self, had_preedit: bool) -> Vec<TextInputEvent> {
        let has_changes = self.preedit.is_some() || self.commit.is_some() || self.delete.is_some();
        let mut events = Vec::with_capacity(5);
        if had_preedit && has_changes {
            events.push(TextInputEvent::composition(""));
            events.push(TextInputEvent::CompositionEnd);
        }
        if let Some((before, after)) = self.delete {
            if before != 0 || after != 0 {
                events.push(TextInputEvent::delete_surrounding_utf8(before, after));
            }
        }
        if let Some(Some(commit)) = self.commit {
            if !commit.is_empty() {
                events.push(TextInputEvent::insert(commit));
            }
        }
        if let Some(Some(preedit)) = self.preedit {
            events.push(TextInputEvent::CompositionUpdate(preedit));
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;

    use super::{
        MAX_SURROUNDING_TEXT_BYTES, WaylandContentType, WaylandCursorRectangle,
        WaylandPreeditError, WaylandTextInputBatch, content_type_from_configuration,
        cursor_rectangle_from_physical, surrounding_text_from_host,
    };
    use dpi::{PhysicalPosition, PhysicalSize};
    use ui_events::text::{TextInputEvent, TextRangeEncoding, TextTargetRange};
    use ui_text_input::{
        CompositionSnapshot, SurroundingTextProvider, SurroundingTextRequest,
        SurroundingTextSnapshot, TextInputConfiguration, TextInputHints, TextInputHost,
        TextInputPurpose, TextInputRect, TextInputSnapshot, TextRangeConverter, TextRangeSlice,
        TextSelection,
    };

    #[derive(Debug)]
    struct StubHost;

    impl TextInputHost for StubHost {
        fn text_input_snapshot(&self) -> TextInputSnapshot {
            TextInputSnapshot::new()
                .with_revision(9)
                .with_document_range(TextTargetRange::utf8_bytes(0, 6))
                .with_selection(TextSelection::utf8_bytes(5, 1))
                .with_composition(
                    CompositionSnapshot::new(TextTargetRange::utf8_bytes(1, 5))
                        .try_with_text("🙂")
                        .expect("matching composition"),
                )
        }
    }

    impl SurroundingTextProvider for StubHost {
        fn surrounding_text(
            &self,
            request: SurroundingTextRequest,
        ) -> Option<SurroundingTextSnapshot> {
            assert_eq!(request.encoding, TextRangeEncoding::Utf8Bytes);
            assert_eq!(request.max_utf8_bytes, MAX_SURROUNDING_TEXT_BYTES);
            SurroundingTextSnapshot::new(
                9,
                TextRangeSlice::new("a🙂b", TextTargetRange::utf8_bytes(0, 6))?,
                TextSelection::utf8_bytes(5, 1),
            )
        }
    }

    impl TextRangeConverter for StubHost {
        fn convert_range(
            &self,
            range: TextTargetRange,
            encoding: TextRangeEncoding,
        ) -> Option<TextTargetRange> {
            (range.encoding == encoding).then_some(range)
        }
    }

    #[test]
    fn surrounding_text_omits_preedit_and_preserves_direction() {
        let text = surrounding_text_from_host(&StubHost, 100, 100).expect("surrounding text");
        assert_eq!(text.revision(), 9);
        assert_eq!(text.text(), "ab");
        assert_eq!(text.cursor(), 1);
        assert_eq!(text.anchor(), 1);
    }

    #[test]
    fn content_type_maps_all_shared_wayland_values() {
        let content = content_type_from_configuration(TextInputConfiguration {
            purpose: TextInputPurpose::Email,
            hints: TextInputHints::COMPLETION
                | TextInputHints::SPELLCHECK
                | TextInputHints::MULTILINE,
            action: None,
        });
        assert_eq!(
            content,
            WaylandContentType {
                hint: 0x203,
                purpose: 6
            }
        );
    }

    #[test]
    fn physical_cursor_rectangle_covers_fractional_logical_bounds() {
        let rect = TextInputRect::new(
            PhysicalPosition::new(15.5, 30.0),
            PhysicalSize::new(2.0, 28.0),
        );
        assert_eq!(
            cursor_rectangle_from_physical(rect, 1.5),
            Some(WaylandCursorRectangle {
                x: 10,
                y: 20,
                width: 2,
                height: 19
            })
        );
    }

    #[test]
    fn done_batch_preserves_protocol_edit_order() {
        let mut batch = WaylandTextInputBatch::default();
        batch
            .set_preedit(Some("🙂"), 0, 4)
            .expect("valid UTF-8 selection");
        batch.set_delete_surrounding_text(2, 1);
        batch.set_commit_string(Some("x"));
        let events = batch.into_events(true);
        assert_eq!(events[0], TextInputEvent::composition(""));
        assert!(matches!(events[1], TextInputEvent::CompositionEnd));
        assert!(matches!(events[2], TextInputEvent::DeleteSurrounding(_)));
        assert_eq!(events[3], TextInputEvent::insert("x"));
        assert!(matches!(events[4], TextInputEvent::CompositionUpdate(_)));
    }

    #[test]
    fn preedit_rejects_partial_hiding_and_split_code_points() {
        let mut batch = WaylandTextInputBatch::default();
        assert_eq!(
            batch.set_preedit(Some("🙂"), -1, 0),
            Err(WaylandPreeditError::PartiallyHiddenCursor)
        );
        assert_eq!(
            batch.set_preedit(Some("🙂"), 0, 2),
            Err(WaylandPreeditError::InvalidCursorRange)
        );
        batch
            .set_preedit(Some("hidden"), -1, -1)
            .expect("hidden cursor pair");
    }

    #[test]
    fn batch_accepts_owned_text_without_std() {
        let mut batch = WaylandTextInputBatch::default();
        let committed = String::from("hello");
        batch.set_commit_string(Some(&committed));
        assert_eq!(batch.into_events(false), [TextInputEvent::insert("hello")]);
    }
}
