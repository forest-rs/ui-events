// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Value adapters between Windows Text Services Framework (TSF) and
//! [`ui-events`].
//!
//! TSF's `ITextStoreACP` API uses signed `LONG` offsets into a UTF-16 stream.
//! This crate owns the portable, allocation-safe translation at that seam:
//!
//! - coherent host state becomes ACP document and selection snapshots;
//! - `GetText`-style requests can split surrogate pairs without manufacturing
//!   invalid Rust strings;
//! - TSF selections preserve their active end and interim-character state;
//! - text replacements produce both [`TextInputEvent`] and `TS_TEXTCHANGE`
//!   values;
//! - screen-space extent and hit-test values have explicit semantics.
//!
//! This is not an `ITextStoreACP` COM implementation. The native Windows layer
//! must still own COM identity, `AdviseSink`, `RequestLock` arbitration,
//! reentrancy, edit/layout notifications, thread/document/context managers,
//! focus, and composition sinks. See `NATIVE_INTEGRATION.md` in this crate for
//! the implementation checklist. Those lifetime and transaction rules cannot
//! be made portable without weakening TSF's contract.
//!
//! [`ui-events`]: https://docs.rs/ui-events/
//! [`TextInputEvent`]: ui_events::text::TextInputEvent

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
use alloc::vec::Vec;

use dpi::PhysicalPosition;
use ui_events::text::{TextInputEvent, TextRangeEncoding, TextTargetRange};
use ui_text_input::{
    TextClosestPositionProvider, TextHitTestProvider, TextInputHost, TextInputRect,
    TextInputSnapshot, TextRangeConverter, TextRangeProvider, TextSelection,
};

/// TSF's sentinel for the end of a document in `GetText` requests.
pub const TSF_DOCUMENT_END: i32 = -1;

/// An ordered range in TSF application character positions (ACP).
///
/// ACP offsets are UTF-16 code-unit offsets for a plain Unicode text store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TsfRange {
    start: i32,
    end: i32,
}

impl TsfRange {
    /// Construct a nonnegative, ordered ACP range.
    pub const fn new(start: i32, end: i32) -> Option<Self> {
        if start < 0 || end < start {
            None
        } else {
            Some(Self { start, end })
        }
    }

    /// Return the first ACP offset in the range.
    pub const fn start(self) -> i32 {
        self.start
    }

    /// Return the exclusive ending ACP offset.
    pub const fn end(self) -> i32 {
        self.end
    }

    /// Convert the range to the portable UTF-16 range type.
    pub fn to_text_range(self) -> Option<TextTargetRange> {
        Some(TextTargetRange::utf16_code_units(
            u32::try_from(self.start).ok()?,
            u32::try_from(self.end).ok()?,
        ))
    }
}

/// Which end of a TSF selection responds to extending-selection operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TsfActiveSelectionEnd {
    /// The selection has no active end, as required for an interim character.
    None,
    /// The active end is the ordered range start.
    Start,
    /// The active end is the ordered range end.
    End,
}

/// Plain values carried by `TS_SELECTION_ACP` and `TS_SELECTIONSTYLE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TsfSelection {
    range: TsfRange,
    active_end: TsfActiveSelectionEnd,
    interim_character: bool,
}

impl TsfSelection {
    /// Construct a validated TSF selection.
    ///
    /// Interim-character selections must be nonempty and have no active end.
    /// The native store must additionally verify against its locked document
    /// that the range covers exactly one character; that cannot be inferred
    /// from ACP offsets alone because a character may use a surrogate pair.
    pub const fn new(
        range: TsfRange,
        active_end: TsfActiveSelectionEnd,
        interim_character: bool,
    ) -> Option<Self> {
        if interim_character
            && (!matches!(active_end, TsfActiveSelectionEnd::None) || range.end == range.start)
        {
            return None;
        }
        Some(Self {
            range,
            active_end,
            interim_character,
        })
    }

    /// Return the ordered ACP range.
    pub const fn range(self) -> TsfRange {
        self.range
    }

    /// Return the active range end.
    pub const fn active_end(self) -> TsfActiveSelectionEnd {
        self.active_end
    }

    /// Return whether this is an interim-character selection.
    pub const fn is_interim_character(self) -> bool {
        self.interim_character
    }

    /// Convert this selection to the portable directional selection.
    ///
    /// `TS_AE_NONE` has no caret direction. It maps to a forward portable
    /// selection while [`Self::is_interim_character`] retains the TSF-only bit.
    pub fn to_text_selection(self) -> Option<TextSelection> {
        let start = u32::try_from(self.range.start).ok()?;
        let end = u32::try_from(self.range.end).ok()?;
        Some(match self.active_end {
            TsfActiveSelectionEnd::Start => TextSelection::utf16_code_units(end, start),
            TsfActiveSelectionEnd::None | TsfActiveSelectionEnd::End => {
                TextSelection::utf16_code_units(start, end)
            }
        })
    }
}

/// Convert the current portable selection to TSF ACP values.
pub fn selection_from_host(
    host: &(impl TextInputHost + TextRangeConverter + ?Sized),
) -> Option<TsfSelection> {
    selection_from_snapshot(host, &host.text_input_snapshot())
}

/// An immutable UTF-16 view of an editor revision.
///
/// Owning UTF-16 values is intentional: a TSF caller may provide a buffer that
/// ends between a surrogate pair, which cannot be represented by a Rust `str`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsfDocumentSnapshot {
    revision: u64,
    text: Vec<u16>,
    selection: Option<TsfSelection>,
}

impl TsfDocumentSnapshot {
    /// Capture a whole-document TSF snapshot from a portable host.
    ///
    /// The provider must return the exact complete document. The host snapshot
    /// is checked again after the text query so a concurrently changed editor
    /// is rejected rather than combining revisions. A native text-store lock
    /// must still prevent mutation while TSF consumes the returned state.
    pub fn from_host(
        host: &(impl TextInputHost + TextRangeProvider + TextRangeConverter + ?Sized),
    ) -> Option<Self> {
        let before = host.text_input_snapshot();
        let document_range = convert_document_range(host, &before)?;
        if document_range.range.start != 0 {
            return None;
        }
        let slice = host.text_for_range(document_range)?;
        if slice.range() != document_range {
            return None;
        }
        let selection = selection_from_snapshot(host, &before);
        let text = slice.text().encode_utf16().collect();
        let after = host.text_input_snapshot();
        if before != after {
            return None;
        }
        Self::new(before.revision(), text, selection)
    }

    /// Construct an owned snapshot from already coherent UTF-16 values.
    pub fn new(revision: u64, text: Vec<u16>, selection: Option<TsfSelection>) -> Option<Self> {
        let length = i32::try_from(text.len()).ok()?;
        if selection.is_some_and(|selection| selection.range.end > length) {
            return None;
        }
        Some(Self {
            revision,
            text,
            selection,
        })
    }

    /// Return the coherent editor revision.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Return the complete UTF-16 document.
    pub fn text(&self) -> &[u16] {
        &self.text
    }

    /// Return the default selection when the host has one.
    pub const fn selection(&self) -> Option<TsfSelection> {
        self.selection
    }

    /// Answer the plain-text portion of an `ITextStoreACP::GetText` request.
    ///
    /// `end` may be [`TSF_DOCUMENT_END`]. `capacity` is measured in `WCHAR`
    /// values. Returned text may intentionally contain one half of a surrogate
    /// pair when that is all the native buffer can hold.
    pub fn get_text(&self, start: i32, end: i32, capacity: u32) -> Option<TsfTextResult> {
        let start = usize::try_from(start).ok()?;
        let end = if end == TSF_DOCUMENT_END {
            self.text.len()
        } else {
            usize::try_from(end).ok()?
        };
        if start > end || end > self.text.len() {
            return None;
        }
        let capacity = usize::try_from(capacity).ok()?;
        let returned_end = start.saturating_add(capacity).min(end);
        Some(TsfTextResult {
            text: self.text[start..returned_end].to_vec(),
            next_acp: i32::try_from(returned_end).ok()?,
        })
    }
}

/// Plain UTF-16 output and continuation position for a TSF text query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsfTextResult {
    text: Vec<u16>,
    next_acp: i32,
}

impl TsfTextResult {
    /// Return the UTF-16 values to copy into `pchPlain`.
    pub fn text(&self) -> &[u16] {
        &self.text
    }

    /// Return the value for `pacpNext`.
    pub const fn next_acp(&self) -> i32 {
        self.next_acp
    }
}

/// Values returned in TSF's `TS_TEXTCHANGE` structure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TsfTextChange {
    /// First ACP offset affected by the edit.
    pub start: i32,
    /// Exclusive end before the edit.
    pub old_end: i32,
    /// Exclusive end after the edit.
    pub new_end: i32,
}

/// A portable replacement event paired with its TSF change result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsfTextReplacement {
    event: TextInputEvent,
    change: TsfTextChange,
}

impl TsfTextReplacement {
    /// Return the editing intent to deliver to the editor.
    pub const fn event(&self) -> &TextInputEvent {
        &self.event
    }

    /// Return the change to report after the editor accepts the exact edit.
    pub const fn change(&self) -> TsfTextChange {
        self.change
    }

    /// Split the adapter result into its event and change values.
    pub fn into_parts(self) -> (TextInputEvent, TsfTextChange) {
        (self.event, self.change)
    }
}

/// Convert a TSF `SetText` replacement into portable editing intent.
///
/// The native store must report [`TsfTextReplacement::change`] only after the
/// editor accepts the event without adjusting its range or inserted text.
pub fn replace_text(start: i32, end: i32, text: impl Into<String>) -> Option<TsfTextReplacement> {
    let range = TsfRange::new(start, end)?;
    let text = text.into();
    let inserted_units = i32::try_from(text.encode_utf16().count()).ok()?;
    let new_end = start.checked_add(inserted_units)?;
    Some(TsfTextReplacement {
        event: TextInputEvent::replace(text, range.to_text_range()?),
        change: TsfTextChange {
            start,
            old_end: end,
            new_end,
        },
    })
}

/// Decode native `WCHAR` values and convert a TSF `SetText` replacement.
///
/// Ill-formed UTF-16 is rejected because [`TextInputEvent`] carries Unicode
/// scalar strings and cannot preserve an unpaired surrogate.
pub fn replace_utf16(start: i32, end: i32, text: &[u16]) -> Option<TsfTextReplacement> {
    replace_text(start, end, String::from_utf16(text).ok()?)
}

/// Convert `InsertTextAtSelection` using the store's current default selection.
pub fn insert_text_at_selection(
    selection: TsfSelection,
    text: impl Into<String>,
) -> Option<TsfTextReplacement> {
    let range = selection.range();
    replace_text(range.start, range.end, text)
}

/// A screen-space integer rectangle suitable for Win32 `RECT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TsfScreenRect {
    /// Left edge in physical screen pixels.
    pub left: i32,
    /// Top edge in physical screen pixels.
    pub top: i32,
    /// Right edge in physical screen pixels.
    pub right: i32,
    /// Bottom edge in physical screen pixels.
    pub bottom: i32,
}

impl TsfScreenRect {
    /// Cover a physical-pixel rectangle using integer Win32 edges.
    ///
    /// The input must already be in virtual-screen coordinates.
    pub fn covering(rect: TextInputRect) -> Option<Self> {
        if rect.size.width < 0.0 || rect.size.height < 0.0 {
            return None;
        }
        let right = rect.origin.x + rect.size.width;
        let bottom = rect.origin.y + rect.size.height;
        Some(Self {
            left: floor_i32(rect.origin.x)?,
            top: floor_i32(rect.origin.y)?,
            right: ceil_i32(right)?,
            bottom: ceil_i32(bottom)?,
        })
    }
}

/// Values returned by `ITextStoreACP::GetTextExt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TsfTextExtent {
    /// Bounding rectangle in physical screen pixels.
    pub rect: TsfScreenRect,
    /// Whether invisible content was excluded from the rectangle.
    pub clipped: bool,
}

/// Convert an exact screen-space hit test to an ACP offset.
///
/// The host must interpret `point` in physical virtual-screen coordinates.
pub fn exact_acp_from_screen_point(
    host: &(impl TextHitTestProvider + ?Sized),
    point: PhysicalPosition<f64>,
) -> Option<i32> {
    i32::try_from(host.text_offset_at_point(point, TextRangeEncoding::Utf16CodeUnits)?).ok()
}

/// Convert a nearest-position screen-space hit test to an ACP offset.
///
/// The host must interpret `point` in physical virtual-screen coordinates.
pub fn nearest_acp_from_screen_point(
    host: &(impl TextClosestPositionProvider + ?Sized),
    point: PhysicalPosition<f64>,
) -> Option<i32> {
    i32::try_from(host.closest_text_offset_to_point(point, TextRangeEncoding::Utf16CodeUnits)?).ok()
}

fn selection_from_snapshot(
    host: &(impl TextRangeConverter + ?Sized),
    snapshot: &TextInputSnapshot,
) -> Option<TsfSelection> {
    let selection = snapshot.selection()?;
    let anchor = convert_offset(host, selection.anchor(), selection.encoding())?;
    let active = convert_offset(host, selection.active(), selection.encoding())?;
    let range = TsfRange::new(
        i32::try_from(anchor.min(active)).ok()?,
        i32::try_from(anchor.max(active)).ok()?,
    )?;
    let active_end = if active < anchor {
        TsfActiveSelectionEnd::Start
    } else {
        TsfActiveSelectionEnd::End
    };
    TsfSelection::new(range, active_end, false)
}

fn convert_document_range(
    host: &(impl TextRangeConverter + ?Sized),
    snapshot: &TextInputSnapshot,
) -> Option<TextTargetRange> {
    let range = snapshot.document_range()?;
    if range.encoding == TextRangeEncoding::Utf16CodeUnits {
        Some(range)
    } else {
        host.convert_range(range, TextRangeEncoding::Utf16CodeUnits)
    }
}

fn convert_offset(
    host: &(impl TextRangeConverter + ?Sized),
    offset: u32,
    encoding: TextRangeEncoding,
) -> Option<u32> {
    if encoding == TextRangeEncoding::Utf16CodeUnits {
        return Some(offset);
    }
    let range = target_range(offset, offset, encoding);
    Some(
        host.convert_range(range, TextRangeEncoding::Utf16CodeUnits)?
            .range
            .start,
    )
}

const fn target_range(start: u32, end: u32, encoding: TextRangeEncoding) -> TextTargetRange {
    match encoding {
        TextRangeEncoding::Utf8Bytes => TextTargetRange::utf8_bytes(start, end),
        TextRangeEncoding::Utf16CodeUnits => TextTargetRange::utf16_code_units(start, end),
        TextRangeEncoding::UnicodeCodePoints => TextTargetRange::unicode_code_points(start, end),
    }
}

fn floor_i32(value: f64) -> Option<i32> {
    if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the value is explicitly bounded to the i32 domain"
    )]
    let truncated = value as i32;
    if value < 0.0 && f64::from(truncated) != value {
        truncated.checked_sub(1)
    } else {
        Some(truncated)
    }
}

fn ceil_i32(value: f64) -> Option<i32> {
    if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "the value is explicitly bounded to the i32 domain"
    )]
    let truncated = value as i32;
    if value > 0.0 && f64::from(truncated) != value {
        truncated.checked_add(1)
    } else {
        Some(truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TSF_DOCUMENT_END, TsfActiveSelectionEnd, TsfDocumentSnapshot, TsfRange, TsfScreenRect,
        TsfSelection, TsfTextChange, exact_acp_from_screen_point, insert_text_at_selection,
        nearest_acp_from_screen_point, replace_text, replace_utf16, selection_from_host,
    };
    use alloc::vec;
    use dpi::{PhysicalPosition, PhysicalSize};
    use ui_events::text::{TextInputEvent, TextRangeEncoding, TextTargetRange};
    use ui_text_input::{
        TextClosestPositionProvider, TextHitTestProvider, TextInputHost, TextInputRect,
        TextInputSnapshot, TextRangeConverter, TextRangeProvider, TextRangeSlice, TextSelection,
    };

    #[derive(Debug)]
    struct StubHost;

    impl TextInputHost for StubHost {
        fn text_input_snapshot(&self) -> TextInputSnapshot {
            TextInputSnapshot::new()
                .with_revision(7)
                .with_document_range(TextTargetRange::utf8_bytes(0, 6))
                .with_selection(TextSelection::utf8_bytes(5, 1))
        }
    }

    impl TextRangeConverter for StubHost {
        fn convert_range(
            &self,
            range: TextTargetRange,
            encoding: TextRangeEncoding,
        ) -> Option<TextTargetRange> {
            if range.encoding == encoding {
                return Some(range);
            }
            if range.encoding != TextRangeEncoding::Utf8Bytes
                || encoding != TextRangeEncoding::Utf16CodeUnits
            {
                return None;
            }
            match (range.range.start, range.range.end) {
                (0, 6) => Some(TextTargetRange::utf16_code_units(0, 4)),
                (1, 1) => Some(TextTargetRange::utf16_code_units(1, 1)),
                (5, 5) => Some(TextTargetRange::utf16_code_units(3, 3)),
                _ => None,
            }
        }
    }

    impl TextRangeProvider for StubHost {
        fn text_for_range(&self, proposed_range: TextTargetRange) -> Option<TextRangeSlice> {
            (proposed_range == TextTargetRange::utf16_code_units(0, 4))
                .then(|| TextRangeSlice::new("a🙂b", proposed_range))?
        }
    }

    impl TextHitTestProvider for StubHost {
        fn text_offset_at_point(
            &self,
            point: PhysicalPosition<f64>,
            encoding: TextRangeEncoding,
        ) -> Option<u32> {
            (point.x == 12.0 && encoding == TextRangeEncoding::Utf16CodeUnits).then_some(2)
        }
    }

    impl TextClosestPositionProvider for StubHost {
        fn closest_text_offset_to_point(
            &self,
            _point: PhysicalPosition<f64>,
            encoding: TextRangeEncoding,
        ) -> Option<u32> {
            (encoding == TextRangeEncoding::Utf16CodeUnits).then_some(4)
        }
    }

    #[test]
    fn selection_preserves_the_active_end() {
        let selection = selection_from_host(&StubHost).expect("selection");
        assert_eq!(selection.range(), TsfRange::new(1, 3).unwrap());
        assert_eq!(selection.active_end(), TsfActiveSelectionEnd::Start);
        assert_eq!(
            selection.to_text_selection(),
            Some(TextSelection::utf16_code_units(3, 1))
        );
    }

    #[test]
    fn interim_character_invariants_are_checked() {
        let one = TsfRange::new(2, 3).unwrap();
        assert!(TsfSelection::new(one, TsfActiveSelectionEnd::None, true).is_some());
        assert!(TsfSelection::new(one, TsfActiveSelectionEnd::End, true).is_none());
        assert!(
            TsfSelection::new(
                TsfRange::new(2, 2).unwrap(),
                TsfActiveSelectionEnd::None,
                true
            )
            .is_none()
        );
    }

    #[test]
    fn whole_document_snapshot_can_split_a_surrogate_pair() {
        let snapshot = TsfDocumentSnapshot::from_host(&StubHost).expect("snapshot");
        assert_eq!(snapshot.revision(), 7);
        assert_eq!(snapshot.text(), &[0x0061, 0xd83d, 0xde42, 0x0062]);
        let first_half = snapshot.get_text(1, TSF_DOCUMENT_END, 1).unwrap();
        assert_eq!(first_half.text(), &[0xd83d]);
        assert_eq!(first_half.next_acp(), 2);
        let remainder = snapshot.get_text(2, TSF_DOCUMENT_END, 8).unwrap();
        assert_eq!(remainder.text(), &[0xde42, 0x0062]);
        assert_eq!(remainder.next_acp(), 4);
        assert!(snapshot.get_text(5, TSF_DOCUMENT_END, 1).is_none());
    }

    #[test]
    fn replacements_report_utf16_change_values() {
        let replacement = replace_text(3, 7, "🙂x").expect("replacement");
        assert_eq!(
            replacement.change(),
            TsfTextChange {
                start: 3,
                old_end: 7,
                new_end: 6,
            }
        );
        assert_eq!(
            replacement.event(),
            &TextInputEvent::replace("🙂x", TextTargetRange::utf16_code_units(3, 7))
        );

        let selection = TsfSelection::new(
            TsfRange::new(8, 10).unwrap(),
            TsfActiveSelectionEnd::Start,
            false,
        )
        .unwrap();
        assert_eq!(
            insert_text_at_selection(selection, "a").unwrap().change(),
            TsfTextChange {
                start: 8,
                old_end: 10,
                new_end: 9,
            }
        );
        assert!(replace_utf16(0, 0, &[0xd800]).is_none());
        assert_eq!(
            replace_utf16(0, 0, &[0xd83d, 0xde42])
                .unwrap()
                .change()
                .new_end,
            2
        );
    }

    #[test]
    fn screen_rect_covers_fractional_and_negative_edges() {
        let rect = TsfScreenRect::covering(TextInputRect::new(
            PhysicalPosition::new(-1.25, 4.75),
            PhysicalSize::new(3.5, 8.1),
        ))
        .unwrap();
        assert_eq!(rect.left, -2);
        assert_eq!(rect.top, 4);
        assert_eq!(rect.right, 3);
        assert_eq!(rect.bottom, 13);
        assert!(
            TsfScreenRect::covering(TextInputRect::new(
                PhysicalPosition::new(0.0, 0.0),
                PhysicalSize::new(-1.0, 2.0),
            ))
            .is_none()
        );
    }

    #[test]
    fn exact_and_nearest_hit_tests_stay_distinct() {
        assert_eq!(
            exact_acp_from_screen_point(&StubHost, PhysicalPosition::new(12.0, 3.0)),
            Some(2)
        );
        assert_eq!(
            exact_acp_from_screen_point(&StubHost, PhysicalPosition::new(99.0, 3.0)),
            None
        );
        assert_eq!(
            nearest_acp_from_screen_point(&StubHost, PhysicalPosition::new(99.0, 3.0)),
            Some(4)
        );
    }

    #[test]
    fn owned_snapshots_accept_native_utf16_values() {
        let selection = TsfSelection::new(
            TsfRange::new(0, 0).unwrap(),
            TsfActiveSelectionEnd::End,
            false,
        );
        let snapshot = TsfDocumentSnapshot::new(2, vec![0xd800], selection).unwrap();
        assert_eq!(snapshot.text(), &[0xd800]);
        assert!(
            TsfDocumentSnapshot::new(
                2,
                vec![0x61],
                TsfSelection::new(
                    TsfRange::new(0, 2).unwrap(),
                    TsfActiveSelectionEnd::End,
                    false,
                ),
            )
            .is_none()
        );
    }
}
