// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Support routines for converting text-input/IME data from the raw Win32 API.
//!
//! The composition-string retrieval here is inlined from `winit-win32`'s
//! `ime.rs` (`ImeContext`), with the resulting strings translated into
//! [`ui_events::text::TextInputEvent`] the same way `ui-events-winit`'s
//! `text.rs` translates `winit::event::Ime`.

use std::ffi::{OsString, c_void};
use std::os::windows::ffi::OsStringExt;
use std::ptr::null_mut;

use ui_events::text::{CompositionState, TextInputEvent, TextInsertEvent, TextRange};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::Ime::{
    ATTR_TARGET_CONVERTED, ATTR_TARGET_NOTCONVERTED, GCS_COMPATTR, GCS_COMPSTR, GCS_CURSORPOS,
    GCS_RESULTSTR, HIMC, ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext,
};

/// A short-lived handle to a window's input context, used to read the
/// in-progress IME composition string.
///
/// Inlined from `winit-win32`'s `ImeContext`, trimmed to the read-side
/// operations needed to translate `WM_IME_COMPOSITION` into
/// [`TextInputEvent`]s; the candidate/composition-window positioning and
/// IME-enablement helpers from the original are not needed here.
struct ImeContext {
    hwnd: HWND,
    himc: HIMC,
}

impl ImeContext {
    /// # Safety
    ///
    /// `hwnd` must be a valid window handle for the window that received the
    /// `WM_IME_*` message currently being processed.
    unsafe fn current(hwnd: HWND) -> Self {
        // SAFETY: `hwnd` is a valid window handle, per this function's contract.
        let himc = unsafe { ImmGetContext(hwnd) };
        Self { hwnd, himc }
    }

    /// Get the in-progress (not yet committed) composition string, along
    /// with the byte-offset range of the "targeted" (currently being
    /// converted) clause, if any.
    ///
    /// # Safety
    ///
    /// The input context must still be valid (the `ImeContext` must not have
    /// outlived the window message it was created for).
    unsafe fn get_composing_text_and_cursor(
        &self,
    ) -> Option<(String, Option<usize>, Option<usize>)> {
        // SAFETY: Upheld by this function's contract.
        let text = unsafe { self.get_composition_string(GCS_COMPSTR) }?;
        // SAFETY: Upheld by this function's contract.
        let attrs = unsafe { self.get_composition_data(GCS_COMPATTR) }.unwrap_or_default();

        let mut first = None;
        let mut last = None;
        let mut boundary_before_char = 0;
        let mut attr_idx = 0;

        for chr in text.chars() {
            let Some(attr) = attrs.get(attr_idx).copied() else {
                break;
            };

            let char_is_targeted =
                attr as u32 == ATTR_TARGET_CONVERTED || attr as u32 == ATTR_TARGET_NOTCONVERTED;

            if first.is_none() && char_is_targeted {
                first = Some(boundary_before_char);
            } else if first.is_some() && last.is_none() && !char_is_targeted {
                last = Some(boundary_before_char);
            }

            boundary_before_char += chr.len_utf8();
            attr_idx += chr.len_utf16();
        }

        if first.is_some() && last.is_none() {
            last = Some(text.len());
        } else if first.is_none() {
            // The IME hasn't split words and selected any clause yet, so try
            // to retrieve the plain cursor position instead.
            // SAFETY: Upheld by this function's contract.
            let cursor = unsafe { self.get_composition_cursor(&text) };
            first = cursor;
            last = cursor;
        }

        Some((text, first, last))
    }

    /// Get the just-committed composition string.
    ///
    /// # Safety
    ///
    /// The input context must still be valid.
    unsafe fn get_composed_text(&self) -> Option<String> {
        // SAFETY: Upheld by this function's contract.
        unsafe { self.get_composition_string(GCS_RESULTSTR) }
    }

    /// # Safety
    ///
    /// The input context must still be valid.
    unsafe fn get_composition_cursor(&self, text: &str) -> Option<usize> {
        // SAFETY: Upheld by this function's contract.
        let cursor = unsafe { ImmGetCompositionStringW(self.himc, GCS_CURSORPOS, null_mut(), 0) };
        (cursor >= 0).then(|| text.chars().take(cursor as _).map(char::len_utf8).sum())
    }

    /// # Safety
    ///
    /// The input context must still be valid.
    unsafe fn get_composition_string(&self, gcs_mode: u32) -> Option<String> {
        // SAFETY: Upheld by this function's contract.
        let data = unsafe { self.get_composition_data(gcs_mode) }?;
        let (prefix, shorts, suffix) = unsafe { data.align_to::<u16>() };
        if prefix.is_empty() && suffix.is_empty() {
            OsString::from_wide(shorts).into_string().ok()
        } else {
            None
        }
    }

    /// # Safety
    ///
    /// The input context must still be valid.
    unsafe fn get_composition_data(&self, gcs_mode: u32) -> Option<Vec<u8>> {
        // SAFETY: Upheld by this function's contract.
        let size = match unsafe { ImmGetCompositionStringW(self.himc, gcs_mode, null_mut(), 0) } {
            0 => return Some(Vec::new()),
            size if size < 0 => return None,
            size => size,
        };

        let mut buf = Vec::<u8>::with_capacity(size as _);
        // SAFETY: `buf` has at least `size` bytes of spare capacity.
        let size = unsafe {
            ImmGetCompositionStringW(
                self.himc,
                gcs_mode,
                buf.as_mut_ptr() as *mut c_void,
                size as _,
            )
        };

        if size < 0 {
            None
        } else {
            // SAFETY: `ImmGetCompositionStringW` just wrote exactly `size` bytes.
            unsafe { buf.set_len(size as _) };
            Some(buf)
        }
    }
}

impl Drop for ImeContext {
    fn drop(&mut self) {
        // SAFETY: `self.hwnd`/`self.himc` were obtained together from a
        // matching `ImmGetContext` call.
        unsafe { ImmReleaseContext(self.hwnd, self.himc) };
    }
}

/// Convert a `WM_IME_COMPOSITION` message's composition data into text input
/// events.
///
/// `lparam` is the message's `lParam`, the bitmask of `GCS_*` flags
/// describing which composition strings changed.
///
/// This mirrors `ui-events-winit`'s `from_winit_ime`: it maps commit and
/// composition-update payloads, leaving the lifecycle notifications
/// (`WM_IME_STARTCOMPOSITION`/`WM_IME_ENDCOMPOSITION`) to the caller, the
/// same way `Ime::Enabled`/`Ime::Disabled` are left to the caller upstream.
///
/// # Safety
///
/// `hwnd` must be a valid handle to the window that received this message.
pub unsafe fn from_win32_ime_composition(hwnd: HWND, lparam: isize) -> Option<Vec<TextInputEvent>> {
    // SAFETY: Upheld by this function's contract.
    let ctx = unsafe { ImeContext::current(hwnd) };
    let flags = lparam as u32;

    if flags & GCS_RESULTSTR != 0 {
        // SAFETY: `ctx` was just created for this message.
        let text = unsafe { ctx.get_composed_text() }?;
        if text.is_empty() {
            return None;
        }
        Some(vec![TextInputEvent::Insert(TextInsertEvent::new(text))])
    } else if flags & GCS_COMPSTR != 0 {
        // SAFETY: `ctx` was just created for this message.
        let (text, first, last) = unsafe { ctx.get_composing_text_and_cursor() }?;
        if text.is_empty() {
            return Some(vec![TextInputEvent::CompositionEnd]);
        }

        let selection = match (first, last) {
            (Some(start), Some(end)) => {
                let start = u32::try_from(start).ok()?;
                let end = u32::try_from(end).ok()?;
                Some(TextRange::new(start, end))
            }
            _ => None,
        };

        let state = CompositionState::new(text);
        let state = match selection {
            Some(selection) => state.try_with_selection(selection)?,
            None => state,
        };
        Some(vec![TextInputEvent::CompositionUpdate(state)])
    } else {
        None
    }
}
