// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Native message responses and translated events.

use ui_events::{keyboard::KeyboardEvent, pointer::PointerEvent, text::TextInputEvent};
use windows_sys::Win32::Foundation::{LPARAM, WPARAM};
use windows_sys::Win32::UI::Input::Ime::ISC_SHOWUICOMPOSITIONWINDOW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    WM_IME_SETCONTEXT, WM_TOUCH, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use super::WindowMessageReducer;

/// A translated input event produced by [`WindowMessageReducer::reduce`].
#[derive(Debug)]
pub enum InputEvent {
    /// Resulting [`KeyboardEvent`].
    Keyboard(KeyboardEvent),
    /// Resulting [`PointerEvent`].
    Pointer(PointerEvent),
    /// Resulting [`TextInputEvent`] values.
    ///
    /// This is a batch because one platform event can map to more than one normalized text event.
    /// For example, committing an active IME composition emits [`TextInputEvent::CompositionEnd`]
    /// followed by the committed [`TextInputEvent::Insert`].
    Text(Vec<TextInputEvent>),
}

/// Native window-procedure disposition for a reduced message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageResponse {
    /// Pass the message to `DefWindowProcW`.
    Forward,
    /// Pass the message to `DefWindowProcW` with these parameters instead of the originals.
    ForwardWithParameters {
        /// Replacement `wParam`.
        wparam: WPARAM,
        /// Replacement `lParam`.
        lparam: LPARAM,
    },
    /// Return the contained `LRESULT` without calling `DefWindowProcW`.
    Consume(isize),
    /// Return the contained `LRESULT` if the application handled a translated event;
    /// otherwise pass the message to `DefWindowProcW`.
    ConsumeIfHandled(isize),
}

/// Result of [`WindowMessageReducer::reduce`].
#[derive(Debug)]
pub struct Reduction {
    /// Zero or more normalized input events.
    pub events: Vec<InputEvent>,
    /// Required native disposition after the application dispatches `events`.
    pub response: MessageResponse,
}

pub(super) const fn response_for_message(
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    ime_allowed: bool,
) -> MessageResponse {
    match msg {
        // The reducer closes the message's touch input handle, so forwarding it would pass an
        // invalid handle to DefWindowProcW.
        WM_TOUCH => MessageResponse::Consume(0),
        // Win32 requires TRUE when an application handles an X-button message.
        WM_XBUTTONDOWN | WM_XBUTTONUP => MessageResponse::ConsumeIfHandled(1),
        // The application renders preedit text, so retain the system candidate UI but suppress
        // the legacy IMM32 composition window.
        WM_IME_SETCONTEXT if ime_allowed => MessageResponse::ForwardWithParameters {
            wparam,
            lparam: ime_context_lparam(lparam),
        },
        WM_IME_SETCONTEXT => MessageResponse::Forward,
        msg if WindowMessageReducer::handles_message(msg) => MessageResponse::ConsumeIfHandled(0),
        _ => MessageResponse::Forward,
    }
}

pub(super) const fn ime_context_lparam(lparam: LPARAM) -> LPARAM {
    lparam & !(ISC_SHOWUICOMPOSITIONWINDOW as LPARAM)
}
