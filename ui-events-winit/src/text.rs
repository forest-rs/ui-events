// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Support routines for converting text-input data from [`winit`].

use alloc::{string::ToString, vec, vec::Vec};

use ui_events::text::{CompositionState, TextInputEvent, TextInsertEvent, TextRange};
use winit::event::Ime;

/// Convert a non-lifecycle [`winit::event::Ime`] payload to text input events.
///
/// This maps commit and preedit payloads. Lifecycle notifications such as
/// [`Ime::Enabled`] and [`Ime::Disabled`] are left to the caller.
///
/// `winit` currently exercises only the common cross-platform subset of
/// [`TextInputEvent`]: committed insertions plus composition updates/end.
/// Android-oriented fields such as explicit document selection updates,
/// surrounding deletes, editor actions, and cursor-placement metadata remain
/// unset here.
pub fn from_winit_ime(ime: &Ime) -> Option<Vec<TextInputEvent>> {
    match ime {
        Ime::Preedit(text, selection) if !text.is_empty() => {
            let selection = selection.and_then(|(start, end)| {
                let start = u32::try_from(start).ok()?;
                let end = u32::try_from(end).ok()?;
                Some(TextRange::new(start, end))
            });
            let state = CompositionState::new(text.to_string());
            let state = match selection {
                Some(selection) => state.try_with_selection(selection)?,
                None => state,
            };
            Some(vec![TextInputEvent::CompositionUpdate(state)])
        }
        Ime::Preedit(_, _) => Some(vec![TextInputEvent::CompositionEnd]),
        Ime::Commit(text) if !text.is_empty() => Some(vec![TextInputEvent::Insert(
            TextInsertEvent::new(text.to_string()),
        )]),
        Ime::Commit(_) | Ime::Enabled | Ime::Disabled => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preedit_maps_to_composition_update() {
        assert_eq!(
            from_winit_ime(&Ime::Preedit("ni".into(), Some((2, 2)))),
            Some(vec![TextInputEvent::CompositionUpdate(
                CompositionState::new("ni").with_selection(TextRange::new(2, 2))
            )])
        );
    }

    #[test]
    fn empty_preedit_maps_to_composition_end() {
        assert_eq!(
            from_winit_ime(&Ime::Preedit("".into(), None)),
            Some(vec![TextInputEvent::CompositionEnd])
        );
    }

    #[test]
    fn preedit_rejects_invalid_selection_offsets() {
        assert_eq!(
            from_winit_ime(&Ime::Preedit("a🙂b".into(), Some((2, 5)))),
            None
        );
    }

    #[test]
    fn commit_maps_to_insert() {
        assert_eq!(
            from_winit_ime(&Ime::Commit("é".into())),
            Some(vec![TextInputEvent::Insert(TextInsertEvent::new("é"))])
        );
    }

    #[test]
    fn winit_does_not_populate_android_specific_text_fields() {
        match from_winit_ime(&Ime::Preedit("ni".into(), Some((2, 2)))).as_deref() {
            Some([TextInputEvent::CompositionUpdate(state)]) => {
                assert_eq!(state.replacement_range, None);
                assert_eq!(
                    state.cursor_placement,
                    ui_events::text::TextCursorPlacement::Unspecified
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }

        match from_winit_ime(&Ime::Commit("é".into())).as_deref() {
            Some([TextInputEvent::Insert(insert)]) => {
                assert_eq!(insert.replacement_range, None);
                assert_eq!(
                    insert.cursor_placement,
                    ui_events::text::TextCursorPlacement::Unspecified
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
