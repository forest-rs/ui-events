// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Support routines for converting text-input/IME data from the raw Win32 API.

use std::{marker::PhantomData, ptr::null_mut};

use ui_events::text::{CompositionState, TextInputEvent, TextInsertEvent, TextRange};
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::UI::Input::Ime::{
    ATTR_TARGET_CONVERTED, ATTR_TARGET_NOTCONVERTED, GCS_COMPATTR, GCS_COMPSTR, GCS_CURSORPOS,
    GCS_RESULTSTR, HIMC, ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext,
};

/// Incrementally decodes the UTF-16 code units delivered by `WM_CHAR`.
#[derive(Debug, Default)]
pub(crate) struct WmCharDecoder {
    pending_high_surrogate: Option<u16>,
}

impl WmCharDecoder {
    /// Decode one `WM_CHAR` UTF-16 code unit.
    pub(crate) fn push(&mut self, code_unit: u16) -> Option<String> {
        if (0xd800..=0xdbff).contains(&code_unit) {
            self.pending_high_surrogate = Some(code_unit);
            return None;
        }

        if (0xdc00..=0xdfff).contains(&code_unit) {
            let high = self.pending_high_surrogate.take()?;
            return String::from_utf16(&[high, code_unit]).ok();
        }

        self.pending_high_surrogate = None;
        char::from_u32(u32::from(code_unit)).map(|ch| ch.to_string())
    }
}

/// Translate one Unicode-window `WM_CHAR` code unit into committed text.
pub(crate) fn from_wm_char(decoder: &mut WmCharDecoder, code_unit: u16) -> Option<TextInputEvent> {
    let text = decoder.push(code_unit)?;
    text.chars()
        .all(|ch| !ch.is_control())
        .then(|| TextInputEvent::Insert(TextInsertEvent::new(text)))
}

/// A short-lived handle to a window's input context, used to read the in-progress
/// IME composition string.
struct ImeContext<'m> {
    hwnd: HWND,
    himc: HIMC,
    _lifetime: PhantomData<&'m ()>,
}

impl<'m> ImeContext<'m> {
    /// # Safety
    ///
    /// `hwnd` must be a valid window handle for the window that received the `WM_IME_*` message
    /// currently being processed.
    unsafe fn current(hwnd: HWND) -> Option<Self> {
        // SAFETY: `hwnd` is a valid window handle.
        let himc = unsafe { ImmGetContext(hwnd) };
        (!himc.is_null()).then_some(Self {
            hwnd,
            himc,
            _lifetime: PhantomData,
        })
    }

    /// Get the in-progress (not yet committed) composition string, along with the byte-offset range
    /// of the "targeted" (currently being converted) clause, if any.
    fn get_composing_text_and_cursor(&self) -> Option<(String, Option<usize>, Option<usize>)> {
        let text = self.get_composition_string(GCS_COMPSTR)?;
        let attrs = self.get_composition_data(GCS_COMPATTR).unwrap_or_default();

        let mut first = None;
        let mut last = None;
        let mut boundary_before_char = 0;
        let mut attr_idx = 0;

        for ch in text.chars() {
            let Some(attr) = attrs.get(attr_idx).copied() else {
                break;
            };

            let char_is_targeted = matches!(
                attr as u32,
                ATTR_TARGET_CONVERTED | ATTR_TARGET_NOTCONVERTED
            );

            if first.is_none() && char_is_targeted {
                first = Some(boundary_before_char);
            } else if first.is_some() && last.is_none() && !char_is_targeted {
                last = Some(boundary_before_char);
            }

            boundary_before_char += ch.len_utf8();
            attr_idx += ch.len_utf16();
        }

        if first.is_some() && last.is_none() {
            last = Some(text.len());
        } else if first.is_none() {
            // The IME hasn't split words and selected any clause yet, so try to retrieve
            // the plain cursor position instead.
            let cursor = self.get_composition_cursor(&text);
            first = cursor;
            last = cursor;
        }

        Some((text, first, last))
    }

    /// Get the just-committed composition string.
    fn get_composed_text(&self) -> Option<String> {
        self.get_composition_string(GCS_RESULTSTR)
    }

    fn get_composition_cursor(&self, text: &str) -> Option<usize> {
        // SAFETY: `self.himc` is valid.
        let cursor = unsafe { ImmGetCompositionStringW(self.himc, GCS_CURSORPOS, null_mut(), 0) };
        (cursor >= 0).then(|| text.chars().take(cursor as _).map(char::len_utf8).sum())
    }

    fn get_composition_string(&self, gcs_mode: u32) -> Option<String> {
        let data = self.get_composition_data(gcs_mode)?;
        // TODO: use this once MSRV is bumped to 1.88
        // let (data, tail) = data.as_chunks::<2>();
        // if !tail.is_empty() {
        //     return None;
        // }
        // let wchs = data.into_iter().map(u16::from_ne_bytes);

        if data.len() % 2 != 0 {
            return None;
        }
        let wchs = data
            .iter()
            .step_by(2)
            .zip(data.iter().skip(1).step_by(2))
            .map(|(a, b)| u16::from_ne_bytes([*a, *b]));

        char::decode_utf16(wchs).collect::<Result<String, _>>().ok()
    }

    fn get_composition_data(&self, gcs_mode: u32) -> Option<Vec<u8>> {
        // SAFETY: `self.himc` is valid.
        let size = match unsafe { ImmGetCompositionStringW(self.himc, gcs_mode, null_mut(), 0) } {
            0 => return Some(Vec::new()),
            size if size < 0 => return None,
            size => size,
        };

        let mut buf = vec![0; size as _];
        // SAFETY: `buf` has at least `size` bytes of spare capacity.
        let size = unsafe {
            ImmGetCompositionStringW(self.himc, gcs_mode, buf.as_mut_ptr().cast(), size as _)
        };

        (size >= 0).then(|| {
            buf.resize(size as _, 0);
            buf
        })
    }
}

impl<'m> Drop for ImeContext<'m> {
    fn drop(&mut self) {
        // SAFETY: `self.himc` was obtained from a matching `ImmGetContext` call with `self.hwnd`.
        unsafe { ImmReleaseContext(self.hwnd, self.himc) };
    }
}

/// Convert a `WM_IME_COMPOSITION` message's composition data into text input events.
///
/// `lparam` is the bitmask of `GCS_*` flags describing which composition strings changed.
///
/// # Safety
///
/// `hwnd` must be a valid handle to the window that received this message.
#[expect(
    clippy::cast_possible_truncation,
    reason = "System provided value, should be no data loss."
)]
pub(crate) unsafe fn from_imm(hwnd: HWND, lparam: LPARAM) -> Option<Vec<TextInputEvent>> {
    // SAFETY: `hwnd` is a valid window handle.
    let ctx = unsafe { ImeContext::current(hwnd) }?;
    let flags = lparam as u32;
    let mut events = Vec::with_capacity(2);

    if flags & GCS_RESULTSTR != 0 {
        if let Some(text) = ctx.get_composed_text().filter(|text| !text.is_empty()) {
            events.push(TextInputEvent::Insert(TextInsertEvent::new(text)));
        }
    }

    if flags & GCS_COMPSTR != 0 {
        if let Some((text, first, last)) = ctx.get_composing_text_and_cursor() {
            if text.is_empty() {
                events.push(TextInputEvent::CompositionEnd);
            } else {
                let mut state = CompositionState::new(text);
                if let Some((start, end)) = first.zip(last) {
                    let selection = u32::try_from(start)
                        .ok()
                        .zip(u32::try_from(end).ok())
                        .and_then(|(start, end)| {
                            state.clone().try_with_selection(TextRange::new(start, end))
                        });
                    if let Some(selected) = selection {
                        state = selected;
                    }
                }
                events.push(TextInputEvent::CompositionUpdate(state));
            }
        }
    }

    (!events.is_empty()).then_some(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wm_char_decodes_surrogate_pairs() {
        let mut decoder = WmCharDecoder::default();

        assert_eq!(decoder.push(0xd83d), None);
        assert_eq!(decoder.push(0xde42).as_deref(), Some("🙂"));
    }

    #[test]
    fn wm_char_filters_editing_control_characters() {
        let mut decoder = WmCharDecoder::default();

        assert_eq!(from_wm_char(&mut decoder, u16::from(b'\x08')), None);
        assert_eq!(
            from_wm_char(&mut decoder, u16::from(b'a')),
            Some(TextInputEvent::Insert(TextInsertEvent::new("a")))
        );
    }
}
