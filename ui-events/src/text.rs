// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # Text input event types
//!
//! This module contains transport-agnostic text-input events for soft keyboards
//! and input methods.
//!
//! These events are intentionally separate from
//! [`crate::keyboard::KeyboardEvent`]. Keyboard events represent key transitions
//! and shortcuts; text events represent editing intent such as committed text,
//! deletion, and composition updates.
//!
//! Plain backward/forward deletion can also appear as
//! [`crate::edit::EditCommandEvent`] when a platform reports the action through
//! a resolved editor-command callback. See [`crate::edit`] for the adapter
//! routing contract.
//!
//! ## Design notes
//!
//! - Text events do not own your text buffer or selection state.
//! - Text events do not imply any physical key identity.
//! - Composition updates are snapshots of the current composing text.
//! - Selection within a composition string uses UTF-8 byte offsets so it can
//!   be sliced directly from Rust strings.
//! - Replacement ranges carry an explicit offset encoding because platforms do
//!   not agree on document indexing units.
//! - `CompositionState::selection` is normalized to UTF-8 byte offsets because
//!   the composing string is carried in the event itself. `TextTargetRange`
//!   keeps its source encoding because it refers to positions in the editor's
//!   document, and `ui-events` does not own the full document text needed to
//!   normalize those offsets safely.
//! - Some platforms, notably Android, also report explicit selection changes,
//!   composing regions, surrounding-delete operations, and editor actions.
//! - Cursor placement after text insertion or composition updates is optional
//!   metadata because some platforms, notably Android, expose it explicitly
//!   while others do not.

use alloc::string::String;
use core::ops::Range;

/// A half-open text range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextRange {
    /// Inclusive start offset.
    pub start: u32,
    /// Exclusive end offset.
    pub end: u32,
}

impl TextRange {
    /// Create a range from explicit offsets.
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

/// The offset encoding used by a platform-provided document range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextRangeEncoding {
    /// Offsets are UTF-8 byte indices.
    Utf8Bytes,
    /// Offsets are UTF-16 code-unit indices.
    Utf16CodeUnits,
    /// Offsets are Unicode code-point indices.
    UnicodeCodePoints,
}

/// A platform-provided document range and its offset encoding.
///
/// Platform provenance:
/// - Apple text APIs commonly use UTF-16 code-unit indices.
/// - Android text APIs may use either UTF-16 code units or Unicode code points.
/// - Wayland text-input uses UTF-8 byte indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextTargetRange {
    /// The underlying range offsets.
    pub range: TextRange,
    /// The encoding used by the platform for those offsets.
    pub encoding: TextRangeEncoding,
}

impl TextTargetRange {
    /// Construct a range whose offsets are UTF-8 byte indices.
    pub const fn utf8_bytes(start: u32, end: u32) -> Self {
        Self {
            range: TextRange::new(start, end),
            encoding: TextRangeEncoding::Utf8Bytes,
        }
    }

    /// Construct a range whose offsets are UTF-16 code-unit indices.
    pub const fn utf16_code_units(start: u32, end: u32) -> Self {
        Self {
            range: TextRange::new(start, end),
            encoding: TextRangeEncoding::Utf16CodeUnits,
        }
    }

    /// Construct a range whose offsets are Unicode code-point indices.
    pub const fn unicode_code_points(start: u32, end: u32) -> Self {
        Self {
            range: TextRange::new(start, end),
            encoding: TextRangeEncoding::UnicodeCodePoints,
        }
    }

    /// Convert this platform-provided range into UTF-8 byte offsets for `text`.
    ///
    /// Returns `None` when the source offsets are out of bounds, reversed, or
    /// do not land on valid character boundaries for the given encoding.
    pub fn to_utf8_range_in(self, text: &str) -> Option<TextRange> {
        let start = self.offset_to_utf8(text, self.range.start)?;
        let end = self.offset_to_utf8(text, self.range.end)?;
        (start <= end).then_some(TextRange::new(start, end))
    }

    /// Convert this platform-provided range into a Rust string slice range.
    ///
    /// Returns `None` when the source offsets are out of bounds, reversed, or
    /// do not land on valid character boundaries for the given encoding.
    ///
    /// # Example
    ///
    /// ```
    /// use ui_events::text::TextTargetRange;
    ///
    /// let mut text = String::from("a🙂b");
    /// let range = TextTargetRange::utf16_code_units(1, 3)
    ///     .to_range_in(&text)
    ///     .expect("valid UTF-16 range");
    /// text.replace_range(range, "x");
    ///
    /// assert_eq!(text, "axb");
    /// ```
    pub fn to_range_in(self, text: &str) -> Option<Range<usize>> {
        let range = self.to_utf8_range_in(text)?;
        Some(range.start as usize..range.end as usize)
    }

    fn offset_to_utf8(self, text: &str, offset: u32) -> Option<u32> {
        match self.encoding {
            TextRangeEncoding::Utf8Bytes => utf8_offset_to_utf8_byte_offset(text, offset),
            TextRangeEncoding::Utf16CodeUnits => utf16_offset_to_utf8_byte_offset(text, offset),
            TextRangeEncoding::UnicodeCodePoints => {
                code_point_offset_to_utf8_byte_offset(text, offset)
            }
        }
    }
}

fn utf8_offset_to_utf8_byte_offset(text: &str, utf8_offset: u32) -> Option<u32> {
    let offset = usize::try_from(utf8_offset).ok()?;
    (offset <= text.len() && text.is_char_boundary(offset)).then_some(utf8_offset)
}

fn utf16_offset_to_utf8_byte_offset(text: &str, utf16_offset: u32) -> Option<u32> {
    if utf16_offset == 0 {
        return Some(0);
    }
    let target = usize::try_from(utf16_offset).ok()?;
    let mut utf16_count = 0_usize;
    for (byte_offset, ch) in text.char_indices() {
        utf16_count = utf16_count.checked_add(ch.len_utf16())?;
        if utf16_count == target {
            let byte_offset = byte_offset.checked_add(ch.len_utf8())?;
            return u32::try_from(byte_offset).ok();
        }
    }
    (utf16_count == target)
        .then(|| u32::try_from(text.len()).ok())
        .flatten()
}

fn code_point_offset_to_utf8_byte_offset(text: &str, code_point_offset: u32) -> Option<u32> {
    if code_point_offset == 0 {
        return Some(0);
    }
    let target = usize::try_from(code_point_offset).ok()?;
    let mut code_point_count = 0_usize;
    for (byte_offset, _) in text.char_indices() {
        if code_point_count == target {
            return u32::try_from(byte_offset).ok();
        }
        code_point_count = code_point_count.checked_add(1)?;
    }
    (code_point_count == target)
        .then(|| u32::try_from(text.len()).ok())
        .flatten()
}

/// A committed text insertion.
///
/// This is the common committed-text path used by web, winit, and Apple text
/// adapters. Android-style adapters may additionally populate
/// [`replacement_range`](Self::replacement_range) and
/// [`cursor_placement`](Self::cursor_placement).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextInsertEvent {
    /// The committed text.
    pub text: String,
    /// The document range to replace, when the platform provides one.
    pub replacement_range: Option<TextTargetRange>,
    /// Cursor-placement metadata associated with this insertion.
    pub cursor_placement: TextCursorPlacement,
}

impl TextInsertEvent {
    /// Create an insertion event from committed text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            replacement_range: None,
            cursor_placement: TextCursorPlacement::Unspecified,
        }
    }

    /// Attach a replacement range to this insertion.
    pub const fn with_replacement_range(mut self, replacement_range: TextTargetRange) -> Self {
        self.replacement_range = Some(replacement_range);
        self
    }

    /// Attach cursor-placement metadata to this insertion.
    pub const fn with_cursor_placement(mut self, cursor_placement: TextCursorPlacement) -> Self {
        self.cursor_placement = cursor_placement;
        self
    }
}

/// A request to delete content around the current selection or insertion point.
///
/// The units of `before_length` and `after_length` are determined by
/// [`Self::encoding`].
///
/// This shape exists primarily for Android-style text protocols such as
/// `InputConnection::deleteSurroundingText`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextDeleteSurroundingEvent {
    /// The number of units to delete before the insertion point.
    pub before_length: u32,
    /// The number of units to delete after the insertion point.
    pub after_length: u32,
    /// The encoding used for both lengths.
    pub encoding: TextRangeEncoding,
}

impl TextDeleteSurroundingEvent {
    /// Construct a surrounding-delete request in UTF-8 byte units.
    pub const fn utf8_bytes(before_length: u32, after_length: u32) -> Self {
        Self {
            before_length,
            after_length,
            encoding: TextRangeEncoding::Utf8Bytes,
        }
    }

    /// Construct a surrounding-delete request in UTF-16 code units.
    pub const fn utf16_code_units(before_length: u32, after_length: u32) -> Self {
        Self {
            before_length,
            after_length,
            encoding: TextRangeEncoding::Utf16CodeUnits,
        }
    }

    /// Construct a surrounding-delete request in Unicode code points.
    pub const fn unicode_code_points(before_length: u32, after_length: u32) -> Self {
        Self {
            before_length,
            after_length,
            encoding: TextRangeEncoding::UnicodeCodePoints,
        }
    }
}

/// A semantic editor action requested by the platform text system.
///
/// This is primarily for Android-style IME action buttons such as "done",
/// "next", and "search".
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextInputAction {
    /// Submit the current field with a "done" action.
    Done,
    /// Submit the current field with a "go" action.
    Go,
    /// Move focus to the next field.
    Next,
    /// Move focus to the previous field.
    Previous,
    /// Submit the current field with a "search" action.
    Search,
    /// Submit the current field with a "send" action.
    Send,
    /// Insert a newline as the preferred editor action.
    Newline,
}

/// Cursor placement associated with a text insertion or composition update.
///
/// This is optional because Android exposes explicit cursor-placement metadata,
/// while the currently modeled web, winit, and Apple adapters do not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TextCursorPlacement {
    /// The platform did not specify cursor placement metadata.
    #[default]
    Unspecified,
    /// The platform specified a relative cursor offset.
    Offset(TextCursorOffset),
}

/// A relative cursor offset associated with inserted or composing text.
///
/// This currently exists to preserve Android cursor-placement semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextCursorOffset {
    /// The offset value in the units specified by [`Self::encoding`].
    pub value: i32,
    /// The encoding used by `value`.
    pub encoding: TextRangeEncoding,
    /// The anchor point from which `value` is interpreted.
    pub relative_to: TextCursorAnchor,
}

impl TextCursorOffset {
    /// Construct a cursor offset with explicit encoding and anchor metadata.
    pub const fn new(
        value: i32,
        encoding: TextRangeEncoding,
        relative_to: TextCursorAnchor,
    ) -> Self {
        Self {
            value,
            encoding,
            relative_to,
        }
    }

    /// Construct a UTF-8 byte cursor offset.
    pub const fn utf8_bytes(value: i32, relative_to: TextCursorAnchor) -> Self {
        Self::new(value, TextRangeEncoding::Utf8Bytes, relative_to)
    }

    /// Construct a UTF-16 code-unit cursor offset.
    pub const fn utf16_code_units(value: i32, relative_to: TextCursorAnchor) -> Self {
        Self::new(value, TextRangeEncoding::Utf16CodeUnits, relative_to)
    }

    /// Construct a Unicode code-point cursor offset.
    pub const fn unicode_code_points(value: i32, relative_to: TextCursorAnchor) -> Self {
        Self::new(value, TextRangeEncoding::UnicodeCodePoints, relative_to)
    }
}

/// The anchor point for interpreting a relative cursor offset.
///
/// These anchors are chosen to describe Android-style relative cursor placement
/// without forcing adapters that do not expose such metadata to invent values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextCursorAnchor {
    /// Relative to the start of the inserted or composing text.
    InsertedTextStart,
    /// Relative to the end of the inserted or composing text.
    InsertedTextEnd,
    /// Relative to the start of the replaced document range.
    ReplacedRangeStart,
    /// Relative to the end of the replaced document range.
    ReplacedRangeEnd,
}

/// A snapshot of the current composition text.
///
/// This is emitted while an IME or soft keyboard is building text that has not
/// yet been committed to the underlying editor content.
///
/// This is the common composition/preedit path used by web, winit, and Apple
/// text adapters. Android-style adapters may additionally populate
/// [`replacement_range`](Self::replacement_range) and
/// [`cursor_placement`](Self::cursor_placement).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionState {
    /// The current composition text.
    pub text: String,
    /// The selection within `text`, expressed as UTF-8 byte offsets.
    pub selection: Option<TextRange>,
    /// The document range to replace, when the platform provides one.
    pub replacement_range: Option<TextTargetRange>,
    /// Cursor-placement metadata associated with this composition update.
    pub cursor_placement: TextCursorPlacement,
}

impl CompositionState {
    /// Create a composition snapshot from the provided text.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            selection: None,
            replacement_range: None,
            cursor_placement: TextCursorPlacement::Unspecified,
        }
    }

    /// Attach a selection within the composing text.
    ///
    /// `selection` must be ordered, in bounds for [`Self::text`], and land on
    /// UTF-8 character boundaries. Use [`Self::try_with_selection`] when
    /// converting unchecked platform offsets.
    pub fn with_selection(mut self, selection: TextRange) -> Self {
        debug_assert!(
            self.is_valid_selection(selection),
            "composition selection must be ordered, in bounds, and on UTF-8 character boundaries"
        );
        self.selection = Some(selection);
        self
    }

    /// Try to attach a selection within the composing text.
    ///
    /// Returns `None` when `selection` is reversed, out of bounds, or does not
    /// land on UTF-8 character boundaries in [`Self::text`].
    pub fn try_with_selection(mut self, selection: TextRange) -> Option<Self> {
        self.is_valid_selection(selection).then(|| {
            self.selection = Some(selection);
            self
        })
    }

    /// Attach a replacement range to this composition update.
    pub const fn with_replacement_range(mut self, replacement_range: TextTargetRange) -> Self {
        self.replacement_range = Some(replacement_range);
        self
    }

    /// Attach cursor-placement metadata to this composition update.
    pub const fn with_cursor_placement(mut self, cursor_placement: TextCursorPlacement) -> Self {
        self.cursor_placement = cursor_placement;
        self
    }

    fn is_valid_selection(&self, selection: TextRange) -> bool {
        TextTargetRange::utf8_bytes(selection.start, selection.end)
            .to_range_in(&self.text)
            .is_some()
    }
}

/// A text-input event.
///
/// These events represent editing intent, not physical key transitions.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInputEvent {
    /// Insert committed text at the current insertion point or selection.
    ///
    /// The text may contain multiple Unicode scalar values or grapheme
    /// clusters, for example when committing emoji or multi-character IME
    /// output.
    Insert(TextInsertEvent),
    /// Delete content immediately before the insertion point.
    DeleteBackward,
    /// Delete content immediately after the insertion point.
    DeleteForward,
    /// Delete content surrounding the current insertion point or selection.
    ///
    /// Primarily produced by Android-style text protocols.
    DeleteSurrounding(TextDeleteSurroundingEvent),
    /// Update the document selection range.
    ///
    /// Primarily produced by Android-style text protocols.
    SetSelection(TextTargetRange),
    /// Update the current composing region in the document.
    ///
    /// Primarily produced by Android-style text protocols.
    SetComposingRegion(TextTargetRange),
    /// Update the current composition text.
    CompositionUpdate(CompositionState),
    /// Clear the current composition.
    CompositionEnd,
    /// Request a semantic editor action such as "search" or "done".
    ///
    /// Primarily produced by Android-style text protocols.
    Action(TextInputAction),
}

impl TextInputEvent {
    /// Construct a committed text insertion.
    pub fn insert(text: impl Into<String>) -> Self {
        Self::Insert(TextInsertEvent::new(text))
    }

    /// Construct a committed text insertion with an explicit replacement range.
    pub fn replace(text: impl Into<String>, replacement_range: TextTargetRange) -> Self {
        Self::Insert(TextInsertEvent::new(text).with_replacement_range(replacement_range))
    }

    /// Construct a composition update with no selection or replacement metadata.
    pub fn composition(text: impl Into<String>) -> Self {
        Self::CompositionUpdate(CompositionState::new(text))
    }

    /// Construct a surrounding-delete request in UTF-8 byte units.
    pub const fn delete_surrounding_utf8(before_length: u32, after_length: u32) -> Self {
        Self::DeleteSurrounding(TextDeleteSurroundingEvent::utf8_bytes(
            before_length,
            after_length,
        ))
    }

    /// Construct a surrounding-delete request in UTF-16 code-unit units.
    pub const fn delete_surrounding_utf16_code_units(
        before_length: u32,
        after_length: u32,
    ) -> Self {
        Self::DeleteSurrounding(TextDeleteSurroundingEvent::utf16_code_units(
            before_length,
            after_length,
        ))
    }

    /// Construct a surrounding-delete request in Unicode code-point units.
    pub const fn delete_surrounding_unicode_code_points(
        before_length: u32,
        after_length: u32,
    ) -> Self {
        Self::DeleteSurrounding(TextDeleteSurroundingEvent::unicode_code_points(
            before_length,
            after_length,
        ))
    }

    /// Construct an explicit document selection update.
    pub const fn set_selection(range: TextTargetRange) -> Self {
        Self::SetSelection(range)
    }

    /// Construct an explicit composing-region update.
    pub const fn set_composing_region(range: TextTargetRange) -> Self {
        Self::SetComposingRegion(range)
    }

    /// Construct a semantic editor action.
    pub const fn action(action: TextInputAction) -> Self {
        Self::Action(action)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CompositionState, TextCursorAnchor, TextCursorOffset, TextCursorPlacement,
        TextDeleteSurroundingEvent, TextInputAction, TextInputEvent, TextInsertEvent, TextRange,
        TextRangeEncoding, TextTargetRange,
    };
    use alloc::string::String;
    use core::ops::Range;

    #[derive(Debug, Default, PartialEq, Eq)]
    struct EditorState {
        text: String,
        selection: Option<TextRange>,
        composing_range: Option<TextRange>,
    }

    impl EditorState {
        fn apply(&mut self, event: TextInputEvent) {
            match event {
                TextInputEvent::Insert(insert) => {
                    let replace = insert
                        .replacement_range
                        .and_then(|range| range.to_range_in(&self.text))
                        .or_else(|| self.selection_range());
                    let insert_len = u32::try_from(insert.text.len()).unwrap();
                    if let Some(range) = replace {
                        let start = u32::try_from(range.start).unwrap();
                        self.text.replace_range(range.clone(), &insert.text);
                        let end = start + insert_len;
                        self.selection = Some(TextRange::new(end, end));
                    } else {
                        self.text.push_str(&insert.text);
                        let end = u32::try_from(self.text.len()).unwrap();
                        self.selection = Some(TextRange::new(end, end));
                    }
                    self.composing_range = None;
                }
                TextInputEvent::DeleteSurrounding(delete) => {
                    let selection = self.selection.expect("selection must be set");
                    assert_eq!(delete.encoding, TextRangeEncoding::Utf8Bytes);
                    let start = selection.start.saturating_sub(delete.before_length);
                    let end = selection.end.saturating_add(delete.after_length);
                    let range = TextRange::new(start, end);
                    let slice = TextTargetRange::utf8_bytes(range.start, range.end)
                        .to_range_in(&self.text)
                        .unwrap();
                    self.text.replace_range(slice, "");
                    self.selection = Some(TextRange::new(start, start));
                    self.composing_range = None;
                }
                TextInputEvent::SetSelection(range) => {
                    self.selection = Some(range.to_utf8_range_in(&self.text).unwrap());
                }
                TextInputEvent::SetComposingRegion(range) => {
                    self.composing_range = Some(range.to_utf8_range_in(&self.text).unwrap());
                }
                TextInputEvent::CompositionUpdate(state) => {
                    let replace = state
                        .replacement_range
                        .and_then(|range| range.to_range_in(&self.text))
                        .or_else(|| self.composing_range_range())
                        .or_else(|| self.selection_range());
                    if let Some(range) = replace {
                        let start = u32::try_from(range.start).unwrap();
                        self.text.replace_range(range.clone(), &state.text);
                        let end = start + u32::try_from(state.text.len()).unwrap();
                        self.composing_range = Some(TextRange::new(start, end));
                        self.selection = state.selection.map(|selection| {
                            TextRange::new(start + selection.start, start + selection.end)
                        });
                    }
                }
                TextInputEvent::CompositionEnd => {
                    self.composing_range = None;
                }
                TextInputEvent::DeleteBackward
                | TextInputEvent::DeleteForward
                | TextInputEvent::Action(_) => {}
            }
        }

        fn selection_range(&self) -> Option<Range<usize>> {
            let selection = self.selection?;
            TextTargetRange::utf8_bytes(selection.start, selection.end).to_range_in(&self.text)
        }

        fn composing_range_range(&self) -> Option<Range<usize>> {
            let range = self.composing_range?;
            TextTargetRange::utf8_bytes(range.start, range.end).to_range_in(&self.text)
        }
    }

    #[test]
    fn insert_text_can_hold_multiple_scalars() {
        assert_eq!(
            TextInputEvent::Insert(
                TextInsertEvent::new("é🙂")
                    .with_replacement_range(TextTargetRange::utf16_code_units(4, 7))
            ),
            TextInputEvent::Insert(
                TextInsertEvent::new("é🙂")
                    .with_replacement_range(TextTargetRange::utf16_code_units(4, 7))
            )
        );
    }

    #[test]
    fn composition_state_preserves_text_selection_and_range() {
        let state = CompositionState::new("ni")
            .with_selection(TextRange::new(0, 2))
            .with_replacement_range(TextTargetRange::utf16_code_units(8, 10));
        assert_eq!(state.text, "ni");
        assert_eq!(state.selection, Some(TextRange::new(0, 2)));
        assert_eq!(
            state.replacement_range,
            Some(TextTargetRange::utf16_code_units(8, 10))
        );
        assert_eq!(state.cursor_placement, TextCursorPlacement::Unspecified);
    }

    #[test]
    fn composition_state_checked_selection_rejects_invalid_ranges() {
        assert_eq!(
            CompositionState::new("a🙂b")
                .try_with_selection(TextRange::new(1, 5))
                .map(|state| state.selection),
            Some(Some(TextRange::new(1, 5)))
        );
        assert_eq!(
            CompositionState::new("a🙂b")
                .try_with_selection(TextRange::new(2, 5))
                .map(|state| state.selection),
            None
        );
        assert_eq!(
            CompositionState::new("abc")
                .try_with_selection(TextRange::new(3, 1))
                .map(|state| state.selection),
            None
        );
    }

    #[test]
    fn target_ranges_can_use_code_point_indices() {
        assert_eq!(
            TextTargetRange::unicode_code_points(3, 5),
            TextTargetRange {
                range: TextRange::new(3, 5),
                encoding: TextRangeEncoding::UnicodeCodePoints,
            }
        );
    }

    #[test]
    fn utf8_target_range_validates_char_boundaries() {
        let text = "a🙂b";
        assert_eq!(
            TextTargetRange::utf8_bytes(1, 5).to_utf8_range_in(text),
            Some(TextRange::new(1, 5))
        );
        assert_eq!(
            TextTargetRange::utf8_bytes(2, 5).to_utf8_range_in(text),
            None
        );
    }

    #[test]
    fn utf16_target_range_converts_surrogate_pairs() {
        let text = "a🙂b";
        assert_eq!(
            TextTargetRange::utf16_code_units(1, 3).to_utf8_range_in(text),
            Some(TextRange::new(1, 5))
        );
        assert_eq!(
            TextTargetRange::utf16_code_units(2, 3).to_utf8_range_in(text),
            None
        );
    }

    #[test]
    fn code_point_target_range_counts_scalars_not_graphemes() {
        let text = "e\u{301}🙂z";
        assert_eq!(
            TextTargetRange::unicode_code_points(0, 2).to_utf8_range_in(text),
            Some(TextRange::new(0, 3))
        );
        assert_eq!(
            TextTargetRange::unicode_code_points(2, 3).to_utf8_range_in(text),
            Some(TextRange::new(3, 7))
        );
    }

    #[test]
    fn target_range_rejects_reversed_and_out_of_bounds_offsets() {
        let text = "abc";
        assert_eq!(
            TextTargetRange::utf8_bytes(3, 1).to_utf8_range_in(text),
            None
        );
        assert_eq!(
            TextTargetRange::utf16_code_units(0, 4).to_utf8_range_in(text),
            None
        );
        assert_eq!(
            TextTargetRange::unicode_code_points(0, 4).to_utf8_range_in(text),
            None
        );
    }

    #[test]
    fn target_range_can_be_used_as_rust_slice_range() {
        let text = "a🙂b";
        assert_eq!(
            TextTargetRange::utf16_code_units(1, 3).to_range_in(text),
            Some(Range { start: 1, end: 5 })
        );
    }

    #[test]
    fn delete_surrounding_event_preserves_lengths_and_encoding() {
        assert_eq!(
            TextDeleteSurroundingEvent::unicode_code_points(2, 1),
            TextDeleteSurroundingEvent {
                before_length: 2,
                after_length: 1,
                encoding: TextRangeEncoding::UnicodeCodePoints,
            }
        );
    }

    #[test]
    fn text_input_event_covers_android_specific_operations() {
        assert_eq!(
            TextInputEvent::DeleteSurrounding(TextDeleteSurroundingEvent::utf16_code_units(3, 2)),
            TextInputEvent::DeleteSurrounding(TextDeleteSurroundingEvent {
                before_length: 3,
                after_length: 2,
                encoding: TextRangeEncoding::Utf16CodeUnits,
            })
        );
        assert_eq!(
            TextInputEvent::SetSelection(TextTargetRange::unicode_code_points(4, 6)),
            TextInputEvent::SetSelection(TextTargetRange {
                range: TextRange::new(4, 6),
                encoding: TextRangeEncoding::UnicodeCodePoints,
            })
        );
        assert_eq!(
            TextInputEvent::SetComposingRegion(TextTargetRange::utf16_code_units(8, 10)),
            TextInputEvent::SetComposingRegion(TextTargetRange::utf16_code_units(8, 10))
        );
        assert_eq!(
            TextInputEvent::Action(TextInputAction::Search),
            TextInputEvent::Action(TextInputAction::Search)
        );
    }

    #[test]
    fn text_input_event_convenience_constructors_match_manual_forms() {
        assert_eq!(
            TextInputEvent::insert("é"),
            TextInputEvent::Insert(TextInsertEvent::new("é"))
        );
        assert_eq!(
            TextInputEvent::replace("x", TextTargetRange::utf16_code_units(4, 5)),
            TextInputEvent::Insert(
                TextInsertEvent::new("x")
                    .with_replacement_range(TextTargetRange::utf16_code_units(4, 5))
            )
        );
        assert_eq!(
            TextInputEvent::composition("ni"),
            TextInputEvent::CompositionUpdate(CompositionState::new("ni"))
        );
        assert_eq!(
            TextInputEvent::delete_surrounding_utf8(2, 1),
            TextInputEvent::DeleteSurrounding(TextDeleteSurroundingEvent::utf8_bytes(2, 1))
        );
        assert_eq!(
            TextInputEvent::delete_surrounding_utf16_code_units(2, 1),
            TextInputEvent::DeleteSurrounding(TextDeleteSurroundingEvent::utf16_code_units(2, 1))
        );
        assert_eq!(
            TextInputEvent::delete_surrounding_unicode_code_points(2, 1),
            TextInputEvent::DeleteSurrounding(TextDeleteSurroundingEvent::unicode_code_points(
                2, 1
            ))
        );
        assert_eq!(
            TextInputEvent::set_selection(TextTargetRange::utf8_bytes(3, 5)),
            TextInputEvent::SetSelection(TextTargetRange::utf8_bytes(3, 5))
        );
        assert_eq!(
            TextInputEvent::set_composing_region(TextTargetRange::utf16_code_units(8, 10)),
            TextInputEvent::SetComposingRegion(TextTargetRange::utf16_code_units(8, 10))
        );
        assert_eq!(
            TextInputEvent::action(TextInputAction::Search),
            TextInputEvent::Action(TextInputAction::Search)
        );
    }

    #[test]
    fn insert_and_composition_events_preserve_cursor_placement() {
        let placement = TextCursorPlacement::Offset(TextCursorOffset::utf16_code_units(
            1,
            TextCursorAnchor::InsertedTextEnd,
        ));
        assert_eq!(
            TextInsertEvent::new("é").with_cursor_placement(placement),
            TextInsertEvent {
                text: "é".into(),
                replacement_range: None,
                cursor_placement: placement,
            }
        );
        assert_eq!(
            CompositionState::new("に").with_cursor_placement(placement),
            CompositionState {
                text: "に".into(),
                selection: None,
                replacement_range: None,
                cursor_placement: placement,
            }
        );
    }

    #[test]
    fn editor_state_example_applies_replacement_selection_and_composition() {
        let mut state = EditorState {
            text: "hello world".into(),
            selection: Some(TextRange::new(6, 11)),
            composing_range: None,
        };

        state.apply(TextInputEvent::Insert(
            TextInsertEvent::new("linebender")
                .with_replacement_range(TextTargetRange::utf8_bytes(6, 11)),
        ));
        assert_eq!(state.text, "hello linebender");
        assert_eq!(state.selection, Some(TextRange::new(16, 16)));

        state.apply(TextInputEvent::SetSelection(TextTargetRange::utf8_bytes(
            16, 16,
        )));
        state.apply(TextInputEvent::DeleteSurrounding(
            TextDeleteSurroundingEvent::utf8_bytes(10, 0),
        ));
        assert_eq!(state.text, "hello ");
        assert_eq!(state.selection, Some(TextRange::new(6, 6)));

        state.apply(TextInputEvent::SetComposingRegion(
            TextTargetRange::utf8_bytes(6, 6),
        ));
        state.apply(TextInputEvent::CompositionUpdate(
            CompositionState::new("に").with_replacement_range(TextTargetRange::utf8_bytes(6, 6)),
        ));
        assert_eq!(state.text, "hello に");
        assert_eq!(state.composing_range, Some(TextRange::new(6, 9)));

        state.apply(TextInputEvent::CompositionEnd);
        assert_eq!(state.composing_range, None);
    }
}
