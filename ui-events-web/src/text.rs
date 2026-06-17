// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Support routines for converting text-input data from [`web_sys`].

use ui_events::text::{CompositionState, TextInputEvent, TextInsertEvent};
use web_sys::wasm_bindgen::JsCast;
use web_sys::{CompositionEvent, Event, InputEvent};

/// Convert a DOM `InputEvent` into a [`TextInputEvent`].
///
/// This is intended for `beforeinput` / `input` handlers. It preserves:
/// - committed insertion text for common `insert*` input types
/// - delete backward / forward intent
/// - composition updates when the browser reports `insertCompositionText`
///
/// This currently exercises only the common cross-platform subset of
/// [`TextInputEvent`]. Android-oriented fields such as explicit document
/// selection updates, surrounding deletes, editor actions, and cursor-placement
/// metadata remain unset here.
pub fn from_web_input_event(event: &InputEvent) -> Option<TextInputEvent> {
    text_event_from_input_type(
        event.input_type().as_str(),
        event.data().as_deref(),
        event.is_composing(),
    )
}

/// Convert a DOM `CompositionEvent` into a [`TextInputEvent`].
///
/// `compositionstart` currently maps to `None` because `ui-events` does not
/// have a distinct composition-start event. `compositionupdate` maps to a
/// composition snapshot, and `compositionend` clears the current composition.
///
/// The `data` carried by `compositionend` is not emitted as committed text.
/// Browsers report the committed text through the paired `beforeinput`/`input`
/// path, usually as `insertFromComposition`, so consumers should wire both DOM
/// input events and composition events.
pub fn from_web_composition_event(event: &CompositionEvent) -> Option<TextInputEvent> {
    text_event_from_composition_type(event.type_().as_str(), event.data().as_deref())
}

/// Convert a generic DOM [`Event`] into a [`TextInputEvent`] when it is either
/// an `InputEvent` or `CompositionEvent`.
pub fn text_event_from_dom_event(event: &Event) -> Option<TextInputEvent> {
    if let Some(input) = event.dyn_ref::<InputEvent>() {
        return from_web_input_event(input);
    }
    event
        .dyn_ref::<CompositionEvent>()
        .and_then(from_web_composition_event)
}

/// Convert raw DOM input-event fields into a [`TextInputEvent`].
pub fn text_event_from_input_type(
    input_type: &str,
    data: Option<&str>,
    is_composing: bool,
) -> Option<TextInputEvent> {
    match input_type {
        "deleteContentBackward" => Some(TextInputEvent::DeleteBackward),
        "deleteContentForward" => Some(TextInputEvent::DeleteForward),
        "insertCompositionText" => Some(TextInputEvent::CompositionUpdate(CompositionState::new(
            data.unwrap_or_default(),
        ))),
        "insertText"
        | "insertReplacementText"
        | "insertLineBreak"
        | "insertParagraph"
        | "insertFromComposition"
        | "insertFromPaste"
        | "insertFromDrop" => {
            let text = data?;
            if text.is_empty() {
                None
            } else if is_composing {
                Some(TextInputEvent::CompositionUpdate(CompositionState::new(
                    text,
                )))
            } else {
                Some(TextInputEvent::Insert(TextInsertEvent::new(text)))
            }
        }
        _ => None,
    }
}

/// Convert raw DOM composition-event fields into a [`TextInputEvent`].
///
/// `compositionend` data is intentionally discarded here; committed text comes
/// from the DOM input-event path.
pub fn text_event_from_composition_type(
    event_type: &str,
    data: Option<&str>,
) -> Option<TextInputEvent> {
    match event_type {
        "compositionstart" => None,
        "compositionupdate" => Some(TextInputEvent::CompositionUpdate(CompositionState::new(
            data.unwrap_or_default(),
        ))),
        "compositionend" => Some(TextInputEvent::CompositionEnd),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_insert_maps_to_text_insert() {
        assert_eq!(
            text_event_from_input_type("insertText", Some("é"), false),
            Some(TextInputEvent::Insert(TextInsertEvent::new("é")))
        );
    }

    #[test]
    fn input_composition_maps_to_snapshot() {
        assert_eq!(
            text_event_from_input_type("insertCompositionText", Some("ni"), true),
            Some(TextInputEvent::CompositionUpdate(CompositionState::new(
                "ni"
            )))
        );
    }

    #[test]
    fn input_delete_maps_to_delete_intent() {
        assert_eq!(
            text_event_from_input_type("deleteContentBackward", None, false),
            Some(TextInputEvent::DeleteBackward)
        );
        assert_eq!(
            text_event_from_input_type("deleteContentForward", None, false),
            Some(TextInputEvent::DeleteForward)
        );
    }

    #[test]
    fn composition_events_map_to_update_and_end() {
        assert_eq!(
            text_event_from_composition_type("compositionupdate", Some("に")),
            Some(TextInputEvent::CompositionUpdate(CompositionState::new(
                "に"
            )))
        );
        assert_eq!(
            text_event_from_composition_type("compositionend", Some("に")),
            Some(TextInputEvent::CompositionEnd)
        );
    }

    #[test]
    fn web_does_not_populate_android_specific_text_fields() {
        match text_event_from_input_type("insertText", Some("é"), false) {
            Some(TextInputEvent::Insert(insert)) => {
                assert_eq!(insert.replacement_range, None);
                assert_eq!(
                    insert.cursor_placement,
                    ui_events::text::TextCursorPlacement::Unspecified
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }

        match text_event_from_input_type("insertCompositionText", Some("ni"), true) {
            Some(TextInputEvent::CompositionUpdate(state)) => {
                assert_eq!(state.replacement_range, None);
                assert_eq!(
                    state.cursor_placement,
                    ui_events::text::TextCursorPlacement::Unspecified
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
