// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Support routines for converting pointer data from the raw Win32 API.

use ui_events::pointer::PointerButton;
use windows_sys::Win32::Foundation::WPARAM;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_XBUTTONDOWN, WM_XBUTTONUP,
};

/// Extract the `XBUTTON1`/`XBUTTON2` identifier from the `wParam` of a
/// `WM_XBUTTONDOWN`/`WM_XBUTTONUP` message.
///
/// Inlined from `winit-win32`'s `util::get_xbutton_wparam` (`HIWORD(wParam)`).
fn get_xbutton_wparam(wparam: WPARAM) -> u16 {
    ((wparam >> 16) & 0xffff) as u16
}

/// Try to make a [`PointerButton`] from a button-related Win32 window
/// message.
///
/// `msg` must be one of `WM_LBUTTONDOWN`, `WM_LBUTTONUP`, `WM_RBUTTONDOWN`,
/// `WM_RBUTTONUP`, `WM_MBUTTONDOWN`, `WM_MBUTTONUP`, `WM_XBUTTONDOWN`, or
/// `WM_XBUTTONUP`. Any other message returns `None`.
///
/// For `WM_XBUTTONDOWN`/`WM_XBUTTONUP`, `wparam` is used to recover which
/// extended button (`XBUTTON1` is back, `XBUTTON2` is forward) was pressed,
/// inlined from `winit-win32`'s handling of those messages in
/// `event_loop.rs`.
pub fn try_from_win32_button(msg: u32, wparam: WPARAM) -> Option<PointerButton> {
    Some(match msg {
        WM_LBUTTONDOWN | WM_LBUTTONUP => PointerButton::Primary,
        WM_RBUTTONDOWN | WM_RBUTTONUP => PointerButton::Secondary,
        WM_MBUTTONDOWN | WM_MBUTTONUP => PointerButton::Auxiliary,
        WM_XBUTTONDOWN | WM_XBUTTONUP => match get_xbutton_wparam(wparam) {
            // XBUTTON1 is defined as back, XBUTTON2 as forward.
            1 => PointerButton::X1,
            2 => PointerButton::X2,
            _ => return None,
        },
        _ => return None,
    })
}
