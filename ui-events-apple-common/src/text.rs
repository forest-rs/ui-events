// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared text-input mapping helpers for Apple adapters.
//!
//! AppKit and UIKit both expose document ranges as UTF-16 location/length
//! pairs. These helpers convert those raw values into the transport-agnostic
//! [`ui_events::text`] event model without depending on AppKit, UIKit, or
//! Objective-C types.

use alloc::string::String;

use ui_events::text::{CompositionState, TextInputEvent, TextInsertEvent, TextTargetRange};

/// Convert an Apple UTF-16 `location`/`length` pair into a target range.
///
/// Returns `None` when `location + length` overflows `u32`.
pub const fn utf16_range_from_location_length(
    location: u32,
    length: u32,
) -> Option<TextTargetRange> {
    match location.checked_add(length) {
        Some(end) => Some(TextTargetRange::utf16_code_units(location, end)),
        None => None,
    }
}

/// Build a committed text insertion event.
pub fn text_insert_event(text: impl Into<String>) -> TextInputEvent {
    TextInputEvent::Insert(TextInsertEvent::new(text))
}

/// Build a committed text insertion event with an Apple UTF-16 replacement range.
///
/// This is suitable for AppKit/UIKit callbacks that provide a document range
/// alongside committed text.
pub fn text_insert_event_with_utf16_replacement(
    text: impl Into<String>,
    location: u32,
    length: u32,
) -> Option<TextInputEvent> {
    let replacement_range = utf16_range_from_location_length(location, length)?;
    Some(TextInputEvent::Insert(
        TextInsertEvent::new(text).with_replacement_range(replacement_range),
    ))
}

/// Build a composition update from composing text and an optional UTF-16
/// selection within that composing text.
///
/// Apple APIs commonly report selected ranges in UTF-16 code units. The core
/// [`CompositionState`] stores selection inside the composing string as UTF-8
/// byte offsets, so this helper validates and converts the selection against
/// `text`.
///
/// `selection_location` and `selection_length` must either both be provided or
/// both be absent. A half-specified selection returns `None`.
pub fn composition_update_event_with_utf16_selection(
    text: impl Into<String>,
    selection_location: Option<u32>,
    selection_length: Option<u32>,
) -> Option<TextInputEvent> {
    let text = text.into();
    let mut state = CompositionState::new(text);
    match (selection_location, selection_length) {
        (Some(location), Some(length)) => {
            let range = utf16_range_from_location_length(location, length)?;
            let selection = range.to_utf8_range_in(&state.text)?;
            state = state.try_with_selection(selection)?;
        }
        (None, None) => {}
        _ => return None,
    }
    Some(TextInputEvent::CompositionUpdate(state))
}

/// Build a composition update with a UTF-16 document replacement range.
///
/// `selection_location`/`selection_length`, when provided, are interpreted
/// within the composing text. `replacement_location`/`replacement_length` are
/// interpreted in the host document.
pub fn composition_update_event_with_utf16_ranges(
    text: impl Into<String>,
    selection_location: Option<u32>,
    selection_length: Option<u32>,
    replacement_location: Option<u32>,
    replacement_length: Option<u32>,
) -> Option<TextInputEvent> {
    let mut state = match composition_update_event_with_utf16_selection(
        text,
        selection_location,
        selection_length,
    )? {
        TextInputEvent::CompositionUpdate(state) => state,
        _ => unreachable!("helper only returns composition updates"),
    };
    match (replacement_location, replacement_length) {
        (Some(location), Some(length)) => {
            state =
                state.with_replacement_range(utf16_range_from_location_length(location, length)?);
        }
        (None, None) => {}
        _ => return None,
    }
    Some(TextInputEvent::CompositionUpdate(state))
}

/// Build a composition-end event.
pub const fn composition_end_event() -> TextInputEvent {
    TextInputEvent::CompositionEnd
}

/// Build a backward-delete text event.
pub const fn delete_backward_event() -> TextInputEvent {
    TextInputEvent::DeleteBackward
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_events::text::{TextRange, TextRangeEncoding};

    #[test]
    fn utf16_ranges_use_location_plus_length() {
        let range = utf16_range_from_location_length(2, 3).expect("valid range");
        assert_eq!(range.range, TextRange::new(2, 5));
        assert_eq!(range.encoding, TextRangeEncoding::Utf16CodeUnits);
        assert_eq!(utf16_range_from_location_length(u32::MAX, 1), None);
    }

    #[test]
    fn insert_event_preserves_utf16_replacement_range() {
        assert_eq!(
            text_insert_event_with_utf16_replacement("x", 1, 2),
            Some(TextInputEvent::replace(
                "x",
                TextTargetRange::utf16_code_units(1, 3)
            ))
        );
    }

    #[test]
    fn composition_selection_converts_to_utf8_offsets() {
        assert_eq!(
            composition_update_event_with_utf16_selection("a🙂b", Some(1), Some(2)),
            Some(TextInputEvent::CompositionUpdate(
                CompositionState::new("a🙂b").with_selection(TextRange::new(1, 5))
            ))
        );
    }

    #[test]
    fn composition_selection_rejects_half_specified_selection() {
        assert_eq!(
            composition_update_event_with_utf16_selection("abc", Some(1), None),
            None
        );
        assert_eq!(
            composition_update_event_with_utf16_selection("abc", None, Some(1)),
            None
        );
    }

    #[test]
    fn composition_update_preserves_document_replacement_range() {
        assert_eq!(
            composition_update_event_with_utf16_ranges("ni", Some(0), Some(2), Some(4), Some(3)),
            Some(TextInputEvent::CompositionUpdate(
                CompositionState::new("ni")
                    .with_selection(TextRange::new(0, 2))
                    .with_replacement_range(TextTargetRange::utf16_code_units(4, 7))
            ))
        );
    }

    #[test]
    fn composition_update_rejects_half_specified_replacement_range() {
        assert_eq!(
            composition_update_event_with_utf16_ranges("ni", Some(0), Some(2), Some(4), None),
            None
        );
        assert_eq!(
            composition_update_event_with_utf16_ranges("ni", Some(0), Some(2), None, Some(3)),
            None
        );
    }

    #[test]
    fn composition_selection_rejects_invalid_utf16_boundaries() {
        assert_eq!(
            composition_update_event_with_utf16_selection("🙂", Some(1), Some(1)),
            None
        );
    }
}
