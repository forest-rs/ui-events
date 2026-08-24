// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Host-side text-input state and query traits for backend adapters.
//!
//! This crate is the reverse side of [`ui-events`]: events carry editing intent
//! from a platform into an application, while these traits let a platform
//! adapter synchronously query coherent editor state.
//!
//! The boundary is deliberately small:
//!
//! - [`TextInputSnapshot`] reports selection and composition state.
//! - [`TextRangeProvider`] and [`SurroundingTextProvider`] expose document text.
//! - Geometry and hit-testing are optional capabilities with explicit semantics.
//! - Backend-specific lifecycle, transactions, locks, and notifications stay in
//!   backend adapters.
//!
//! That split accommodates AppKit and UIKit queries without forcing Wayland's
//! double-buffering, Android's input-connection lifecycle, or Windows TSF's
//! document locks into every editor.
//!
//! [`ui-events`]: https://docs.rs/ui-events/
// LINEBENDER LINT SET - lib.rs - v3
// See https://linebender.org/wiki/canonical-lints/
// These lints shouldn't apply to examples or tests.
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
// These lints shouldn't apply to examples.
#![warn(clippy::print_stdout, clippy::print_stderr)]
// Targeting e.g. 32-bit means structs containing usize can give false positives for 64-bit.
#![cfg_attr(target_pointer_width = "64", warn(clippy::trivially_copy_pass_by_ref))]
// END LINEBENDER LINT SET
#![no_std]

extern crate alloc;

use alloc::string::String;
use core::ops::{BitOr, BitOrAssign};

use dpi::{PhysicalPosition, PhysicalSize};
use ui_events::text::{TextInputAction, TextRange, TextRangeEncoding, TextTargetRange};

/// A rectangle in physical pixels used for text-input geometry.
///
/// The coordinate system matches `ui-events`: the origin is at the top left and
/// the Y axis increases downward. The coordinate space is backend-defined;
/// adapters perform view, window, surface, or screen conversion as required.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextInputRect {
    /// Rectangle origin in physical pixels.
    pub origin: PhysicalPosition<f64>,
    /// Rectangle size in physical pixels.
    pub size: PhysicalSize<f64>,
}

impl TextInputRect {
    /// Construct a rectangle from its origin and size.
    pub const fn new(origin: PhysicalPosition<f64>, size: PhysicalSize<f64>) -> Self {
        Self { origin, size }
    }
}

/// A directional document selection.
///
/// `anchor` is the stationary end and `active` is the end containing the caret.
/// Keeping both ends preserves selection direction for protocols such as
/// Wayland text-input and Windows TSF.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextSelection {
    anchor: u32,
    active: u32,
    encoding: TextRangeEncoding,
}

impl TextSelection {
    /// Construct a selection with explicit offset encoding.
    pub const fn new(anchor: u32, active: u32, encoding: TextRangeEncoding) -> Self {
        Self {
            anchor,
            active,
            encoding,
        }
    }

    /// Construct a selection whose offsets are UTF-8 byte indices.
    pub const fn utf8_bytes(anchor: u32, active: u32) -> Self {
        Self::new(anchor, active, TextRangeEncoding::Utf8Bytes)
    }

    /// Construct a selection whose offsets are UTF-16 code-unit indices.
    pub const fn utf16_code_units(anchor: u32, active: u32) -> Self {
        Self::new(anchor, active, TextRangeEncoding::Utf16CodeUnits)
    }

    /// Construct a selection whose offsets are Unicode code-point indices.
    pub const fn unicode_code_points(anchor: u32, active: u32) -> Self {
        Self::new(anchor, active, TextRangeEncoding::UnicodeCodePoints)
    }

    /// Return the stationary end of the selection.
    pub const fn anchor(self) -> u32 {
        self.anchor
    }

    /// Return the active, caret-bearing end of the selection.
    pub const fn active(self) -> u32 {
        self.active
    }

    /// Return the offset encoding used by both ends.
    pub const fn encoding(self) -> TextRangeEncoding {
        self.encoding
    }

    /// Return whether this selection is a caret rather than a span.
    pub const fn is_collapsed(self) -> bool {
        self.anchor == self.active
    }

    /// Return the ordered range covered by the selection.
    pub const fn ordered_range(self) -> TextTargetRange {
        let (start, end) = if self.anchor <= self.active {
            (self.anchor, self.active)
        } else {
            (self.active, self.anchor)
        };
        target_range(start, end, self.encoding)
    }
}

/// A coherent snapshot of the current composition.
///
/// The document range is always present. Text is optional because some editors
/// can cheaply expose the marked range but prefer to serve its content through
/// [`TextRangeProvider`]. When text is attached, its encoded length must equal
/// the composition range length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionSnapshot {
    range: TextTargetRange,
    text: Option<String>,
}

impl CompositionSnapshot {
    /// Construct a composition snapshot for a document range.
    pub const fn new(range: TextTargetRange) -> Self {
        Self { range, text: None }
    }

    /// Attach the composition text after validating it against the range.
    ///
    /// Returns `None` for a reversed range or when the encoded text length does
    /// not match the document span.
    pub fn try_with_text(mut self, text: impl Into<String>) -> Option<Self> {
        let text = text.into();
        range_matches_text(self.range, &text)?;
        self.text = Some(text);
        Some(self)
    }

    /// Return the composition range in the document.
    pub const fn range(&self) -> TextTargetRange {
        self.range
    }

    /// Return the composition text when it was captured atomically.
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

/// An atomic snapshot of the editor state needed by text-input backends.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextInputSnapshot {
    revision: u64,
    document_range: Option<TextTargetRange>,
    selection: Option<TextSelection>,
    composition: Option<CompositionSnapshot>,
}

impl TextInputSnapshot {
    /// Construct an empty snapshot at revision zero.
    pub const fn new() -> Self {
        Self {
            revision: 0,
            document_range: None,
            selection: None,
            composition: None,
        }
    }

    /// Attach a monotonically changing host revision.
    ///
    /// Adapters may use this to avoid resending unchanged state. Revision
    /// ordering and wraparound remain host-defined.
    pub const fn with_revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }

    /// Attach the editable document range.
    pub const fn with_document_range(mut self, range: TextTargetRange) -> Self {
        self.document_range = Some(range);
        self
    }

    /// Attach the current directional selection.
    pub const fn with_selection(mut self, selection: TextSelection) -> Self {
        self.selection = Some(selection);
        self
    }

    /// Attach the current composition.
    pub fn with_composition(mut self, composition: CompositionSnapshot) -> Self {
        self.composition = Some(composition);
        self
    }

    /// Return the host revision.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Return the editable document range when available.
    pub const fn document_range(&self) -> Option<TextTargetRange> {
        self.document_range
    }

    /// Return the current directional selection when available.
    pub const fn selection(&self) -> Option<TextSelection> {
        self.selection
    }

    /// Return the current composition when available.
    pub const fn composition(&self) -> Option<&CompositionSnapshot> {
        self.composition.as_ref()
    }
}

/// Text returned for an exact or adjusted document range.
///
/// Keeping the actual range beside the text lets adapters correctly answer APIs
/// such as AppKit's `attributedSubstringForProposedRange:actualRange:`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRangeSlice {
    text: String,
    range: TextTargetRange,
}

impl TextRangeSlice {
    /// Construct a validated text slice.
    ///
    /// Returns `None` when `range` is reversed or its encoded length differs
    /// from the supplied text.
    pub fn new(text: impl Into<String>, range: TextTargetRange) -> Option<Self> {
        let text = text.into();
        range_matches_text(range, &text)?;
        Some(Self { text, range })
    }

    /// Return the text content.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Return the actual document range represented by the text.
    pub const fn range(&self) -> TextTargetRange {
        self.range
    }

    /// Consume the slice into its text and range.
    pub fn into_parts(self) -> (String, TextTargetRange) {
        (self.text, self.range)
    }
}

/// A bounded surrounding-text snapshot with a directional selection.
///
/// The selection is expressed in absolute document offsets. Helpers expose the
/// corresponding offsets relative to `text`, as required by Wayland and
/// Android surrounding-text protocols.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurroundingTextSnapshot {
    revision: u64,
    text: TextRangeSlice,
    selection: TextSelection,
}

impl SurroundingTextSnapshot {
    /// Construct a validated surrounding-text snapshot.
    ///
    /// Returns `None` unless both selection ends use the text range's encoding,
    /// fall within the returned text, and land on valid encoded boundaries.
    pub fn new(revision: u64, text: TextRangeSlice, selection: TextSelection) -> Option<Self> {
        let range = text.range();
        if selection.encoding() != range.encoding {
            return None;
        }
        let anchor = relative_offset(range, selection.anchor())?;
        let active = relative_offset(range, selection.active())?;
        validate_encoded_boundary(text.text(), anchor, range.encoding)?;
        validate_encoded_boundary(text.text(), active, range.encoding)?;
        Some(Self {
            revision,
            text,
            selection,
        })
    }

    /// Return the host revision captured with the surrounding text.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Return the bounded text and its absolute document range.
    pub const fn text(&self) -> &TextRangeSlice {
        &self.text
    }

    /// Return the absolute directional selection.
    pub const fn selection(&self) -> TextSelection {
        self.selection
    }

    /// Return the selection anchor relative to the start of the returned text.
    pub fn relative_anchor(&self) -> u32 {
        self.selection.anchor() - self.text.range().range.start
    }

    /// Return the active cursor relative to the start of the returned text.
    pub fn relative_active(&self) -> u32 {
        self.selection.active() - self.text.range().range.start
    }
}

/// Limits for a surrounding-text query.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SurroundingTextRequest {
    /// Desired units before the selection.
    pub before_length: u32,
    /// Desired units after the selection.
    pub after_length: u32,
    /// Encoding used by the requested lengths and returned offsets.
    pub encoding: TextRangeEncoding,
    /// Maximum UTF-8 payload size accepted by the backend.
    pub max_utf8_bytes: u32,
}

impl SurroundingTextRequest {
    /// Construct a surrounding-text request.
    pub const fn new(
        before_length: u32,
        after_length: u32,
        encoding: TextRangeEncoding,
        max_utf8_bytes: u32,
    ) -> Self {
        Self {
            before_length,
            after_length,
            encoding,
            max_utf8_bytes,
        }
    }
}

/// A rectangle covering an exact document subrange.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRangeRect {
    /// The actual range covered by `rect`.
    pub range: TextTargetRange,
    /// The rectangle covering `range`.
    pub rect: TextInputRect,
}

impl TextRangeRect {
    /// Construct a range rectangle.
    pub const fn new(range: TextTargetRange, rect: TextInputRect) -> Self {
        Self { range, rect }
    }
}

/// The primary kind of content expected by a text field.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextInputPurpose {
    /// General text input.
    #[default]
    Normal,
    /// Alphabetic input.
    Alpha,
    /// Decimal digits only.
    Digits,
    /// A signed or decimal number.
    Number,
    /// A telephone number.
    Phone,
    /// A URL.
    Url,
    /// An email address.
    Email,
    /// A person's name.
    Name,
    /// A password.
    Password,
    /// A numeric PIN.
    Pin,
    /// A date.
    Date,
    /// A time.
    Time,
    /// A date and time.
    DateTime,
    /// Terminal input.
    Terminal,
}

/// Backend-independent hints that refine text-input behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextInputHints(u16);

impl TextInputHints {
    /// No special hints.
    pub const NONE: Self = Self(0);
    /// Offer completion suggestions.
    pub const COMPLETION: Self = Self(1 << 0);
    /// Offer spelling correction.
    pub const SPELLCHECK: Self = Self(1 << 1);
    /// Automatically capitalize sentence starts.
    pub const AUTO_CAPITALIZATION: Self = Self(1 << 2);
    /// Prefer lowercase input.
    pub const LOWERCASE: Self = Self(1 << 3);
    /// Prefer uppercase input.
    pub const UPPERCASE: Self = Self(1 << 4);
    /// Prefer title-case input.
    pub const TITLECASE: Self = Self(1 << 5);
    /// Hide entered text visually.
    pub const HIDDEN_TEXT: Self = Self(1 << 6);
    /// Treat input as sensitive and avoid persistence.
    pub const SENSITIVE_DATA: Self = Self(1 << 7);
    /// Prefer Latin characters.
    pub const LATIN: Self = Self(1 << 8);
    /// Allow multiple lines.
    pub const MULTILINE: Self = Self(1 << 9);

    /// Return whether every flag in `other` is present.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for TextInputHints {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for TextInputHints {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Configuration advertised by the focused text field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextInputConfiguration {
    /// Primary content purpose.
    pub purpose: TextInputPurpose,
    /// Behavioral hints.
    pub hints: TextInputHints,
    /// Preferred action for the platform's action key.
    pub action: Option<TextInputAction>,
}

/// Core text-input state that a backend can snapshot from the host.
pub trait TextInputHost {
    /// Return a coherent snapshot of current editor state.
    fn text_input_snapshot(&self) -> TextInputSnapshot;

    /// Return configuration for the focused field.
    fn text_input_configuration(&self) -> TextInputConfiguration {
        TextInputConfiguration::default()
    }
}

/// Optional capability for fetching document text for a proposed range.
pub trait TextRangeProvider {
    /// Return text for all or an adjusted subset of `proposed_range`.
    fn text_for_range(&self, proposed_range: TextTargetRange) -> Option<TextRangeSlice>;
}

/// Optional capability for fetching bounded text around the selection.
pub trait SurroundingTextProvider {
    /// Return surrounding text satisfying as much of `request` as practical.
    fn surrounding_text(&self, request: SurroundingTextRequest) -> Option<SurroundingTextSnapshot>;
}

/// Optional capability for converting document ranges between offset encodings.
pub trait TextRangeConverter {
    /// Convert `range` into `encoding` while preserving its document span.
    fn convert_range(
        &self,
        range: TextTargetRange,
        encoding: TextRangeEncoding,
    ) -> Option<TextTargetRange>;
}

/// Optional capability for caret and range geometry.
pub trait TextGeometryProvider {
    /// Return the insertion caret rectangle.
    fn caret_rect(&self) -> Option<TextInputRect>;

    /// Return the first rectangle and exact subrange covered for `range`.
    fn first_rect_for_range(&self, range: TextTargetRange) -> Option<TextRangeRect>;
}

/// Optional capability for exact character hit-testing.
pub trait TextHitTestProvider {
    /// Return the offset of the character containing `point`.
    ///
    /// Return `None` when the point is outside every character rectangle.
    fn text_offset_at_point(
        &self,
        point: PhysicalPosition<f64>,
        encoding: TextRangeEncoding,
    ) -> Option<u32>;
}

/// Optional capability for finding the closest document position to a point.
pub trait TextClosestPositionProvider {
    /// Return the closest document offset to `point`.
    fn closest_text_offset_to_point(
        &self,
        point: PhysicalPosition<f64>,
        encoding: TextRangeEncoding,
    ) -> Option<u32>;
}

/// Return the previous UTF-8 character boundary before `offset`.
pub fn previous_utf8_boundary(text: &str, offset: u32) -> Option<u32> {
    let offset = usize::try_from(offset).ok()?;
    let prefix = text.get(..offset)?;
    if prefix.is_empty() {
        return None;
    }
    prefix
        .char_indices()
        .last()
        .and_then(|(index, _)| u32::try_from(index).ok())
}

/// Return the next UTF-8 character boundary after `offset`.
pub fn next_utf8_boundary(text: &str, offset: u32) -> Option<u32> {
    let offset = usize::try_from(offset).ok()?;
    let suffix = text.get(offset..)?;
    if suffix.is_empty() {
        return None;
    }
    let next = suffix
        .char_indices()
        .nth(1)
        .and_then(|(index, _)| offset.checked_add(index))
        .unwrap_or(text.len());
    u32::try_from(next).ok()
}

/// Convert a UTF-8 byte offset into another offset encoding for `text`.
pub fn utf8_offset_in_encoding(
    text: &str,
    utf8_offset: u32,
    encoding: TextRangeEncoding,
) -> Option<u32> {
    let offset = usize::try_from(utf8_offset).ok()?;
    let prefix = text.get(..offset)?;
    match encoding {
        TextRangeEncoding::Utf8Bytes => Some(utf8_offset),
        TextRangeEncoding::Utf16CodeUnits => u32::try_from(prefix.encode_utf16().count()).ok(),
        TextRangeEncoding::UnicodeCodePoints => u32::try_from(prefix.chars().count()).ok(),
    }
}

/// Convert a UTF-8 byte range into another offset encoding for `text`.
pub fn utf8_range_to_target_range(
    text: &str,
    range: TextRange,
    encoding: TextRangeEncoding,
) -> Option<TextTargetRange> {
    let start = utf8_offset_in_encoding(text, range.start, encoding)?;
    let end = utf8_offset_in_encoding(text, range.end, encoding)?;
    (start <= end).then(|| target_range(start, end, encoding))
}

const fn target_range(start: u32, end: u32, encoding: TextRangeEncoding) -> TextTargetRange {
    match encoding {
        TextRangeEncoding::Utf8Bytes => TextTargetRange::utf8_bytes(start, end),
        TextRangeEncoding::Utf16CodeUnits => TextTargetRange::utf16_code_units(start, end),
        TextRangeEncoding::UnicodeCodePoints => TextTargetRange::unicode_code_points(start, end),
    }
}

fn range_matches_text(range: TextTargetRange, text: &str) -> Option<()> {
    let length = range.range.end.checked_sub(range.range.start)?;
    (encoded_length(text, range.encoding)? == length).then_some(())
}

fn encoded_length(text: &str, encoding: TextRangeEncoding) -> Option<u32> {
    let length = match encoding {
        TextRangeEncoding::Utf8Bytes => text.len(),
        TextRangeEncoding::Utf16CodeUnits => text.encode_utf16().count(),
        TextRangeEncoding::UnicodeCodePoints => text.chars().count(),
    };
    u32::try_from(length).ok()
}

fn relative_offset(range: TextTargetRange, offset: u32) -> Option<u32> {
    let relative = offset.checked_sub(range.range.start)?;
    (offset <= range.range.end).then_some(relative)
}

fn validate_encoded_boundary(text: &str, offset: u32, encoding: TextRangeEncoding) -> Option<()> {
    target_range(offset, offset, encoding)
        .to_utf8_range_in(text)
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{
        CompositionSnapshot, SurroundingTextSnapshot, TextInputConfiguration, TextInputHints,
        TextInputHost, TextInputSnapshot, TextRangeSlice, TextSelection, next_utf8_boundary,
        previous_utf8_boundary, utf8_offset_in_encoding, utf8_range_to_target_range,
    };
    use ui_events::text::{TextRange, TextRangeEncoding, TextTargetRange};

    #[derive(Debug)]
    struct StubHost;

    impl TextInputHost for StubHost {
        fn text_input_snapshot(&self) -> TextInputSnapshot {
            TextInputSnapshot::new()
                .with_revision(7)
                .with_document_range(TextTargetRange::utf8_bytes(0, 6))
                .with_selection(TextSelection::utf8_bytes(5, 1))
                .with_composition(
                    CompositionSnapshot::new(TextTargetRange::utf8_bytes(1, 5))
                        .try_with_text("🙂")
                        .expect("composition range matches text"),
                )
        }
    }

    #[test]
    fn snapshot_keeps_composition_range_and_selection_direction() {
        let snapshot = StubHost.text_input_snapshot();
        assert_eq!(snapshot.revision(), 7);
        let selection = snapshot.selection().expect("selection");
        assert_eq!(selection.anchor(), 5);
        assert_eq!(selection.active(), 1);
        assert_eq!(selection.ordered_range(), TextTargetRange::utf8_bytes(1, 5));
        let composition = snapshot.composition().expect("composition");
        assert_eq!(composition.range(), TextTargetRange::utf8_bytes(1, 5));
        assert_eq!(composition.text(), Some("🙂"));
    }

    #[test]
    fn composition_rejects_mismatched_range_lengths() {
        assert_eq!(
            CompositionSnapshot::new(TextTargetRange::utf16_code_units(0, 1)).try_with_text("🙂"),
            None
        );
        assert!(
            CompositionSnapshot::new(TextTargetRange::utf16_code_units(0, 2))
                .try_with_text("🙂")
                .is_some()
        );
    }

    #[test]
    fn text_range_slice_reports_the_actual_validated_range() {
        assert_eq!(
            TextRangeSlice::new("🙂", TextTargetRange::utf16_code_units(4, 6))
                .expect("matching UTF-16 range")
                .range(),
            TextTargetRange::utf16_code_units(4, 6)
        );
        assert_eq!(
            TextRangeSlice::new("🙂", TextTargetRange::utf16_code_units(4, 5)),
            None
        );
    }

    #[test]
    fn surrounding_text_preserves_relative_cursor_and_anchor() {
        let text = TextRangeSlice::new("a🙂b", TextTargetRange::utf8_bytes(10, 16))
            .expect("matching UTF-8 range");
        let snapshot = SurroundingTextSnapshot::new(9, text, TextSelection::utf8_bytes(15, 11))
            .expect("selection lies on boundaries inside text");
        assert_eq!(snapshot.relative_anchor(), 5);
        assert_eq!(snapshot.relative_active(), 1);
        assert_eq!(snapshot.revision(), 9);
    }

    #[test]
    fn surrounding_text_rejects_wrong_encoding_range_and_boundaries() {
        let make_text = || {
            TextRangeSlice::new("a🙂b", TextTargetRange::utf8_bytes(10, 16))
                .expect("matching UTF-8 range")
        };
        assert_eq!(
            SurroundingTextSnapshot::new(0, make_text(), TextSelection::utf16_code_units(1, 1)),
            None
        );
        assert_eq!(
            SurroundingTextSnapshot::new(0, make_text(), TextSelection::utf8_bytes(9, 10)),
            None
        );
        assert_eq!(
            SurroundingTextSnapshot::new(0, make_text(), TextSelection::utf8_bytes(12, 15)),
            None
        );
    }

    #[test]
    fn text_hints_compose_without_a_dependency() {
        let hints = TextInputHints::COMPLETION | TextInputHints::MULTILINE;
        assert!(hints.contains(TextInputHints::COMPLETION));
        assert!(hints.contains(TextInputHints::MULTILINE));
        assert!(!hints.contains(TextInputHints::SENSITIVE_DATA));
        assert_eq!(
            StubHost.text_input_configuration(),
            TextInputConfiguration::default()
        );
    }

    #[test]
    fn utf8_helpers_convert_offsets_ranges_and_boundaries() {
        let text = "a🙂b";
        assert_eq!(previous_utf8_boundary(text, 5), Some(1));
        assert_eq!(next_utf8_boundary(text, 1), Some(5));
        assert_eq!(
            utf8_offset_in_encoding(text, 5, TextRangeEncoding::Utf16CodeUnits),
            Some(3)
        );
        assert_eq!(
            utf8_range_to_target_range(
                text,
                TextRange::new(1, 5),
                TextRangeEncoding::Utf16CodeUnits,
            ),
            Some(TextTargetRange::utf16_code_units(1, 3))
        );
    }
}
