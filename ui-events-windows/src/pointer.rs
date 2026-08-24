// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Support routines for converting pointer data from the raw Win32 API.

use dpi::PhysicalPosition;
use ui_events::pointer::{PointerButton, PointerId};
use windows_sys::Win32::Foundation::WPARAM;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_XBUTTONDOWN, WM_XBUTTONUP,
};

/// Manages repetition state of pointer events.
#[derive(Debug)]
pub(crate) struct TapState {
    /// ID of the pointer it tracks.
    pub(crate) pointer_id: Option<PointerId>,
    /// Nanosecond timestamp when the tap went Down.
    pub(crate) down_time: u64,
    /// Nanosecond timestamp when the tap went Up.
    ///
    /// Resets to `down_time` when tap goes Down.
    pub(crate) up_time: u64,
    /// The local tap count as of the last Down phase.
    pub(crate) count: u8,
    /// x coordinate.
    pub(crate) x: f64,
    /// y coordinate.
    pub(crate) y: f64,
}

impl TapState {
    pub(crate) fn pointer_is(&self, pointer_id: Option<PointerId>) -> bool {
        self.pointer_id == pointer_id
    }

    pub(crate) fn is_down(&self) -> bool {
        self.down_time == self.up_time
    }

    pub(crate) fn is_in_range(&self, position: PhysicalPosition<f64>, slop: f64) -> bool {
        (self.x - position.x).hypot(self.y - position.y) < slop
    }

    pub(crate) fn is_valid_for(&self, time: u64) -> bool {
        self.up_time + 500_000_000 > time
    }
}

/// Try to make a [`PointerButton`] from a button-related Win32 window message.
pub(crate) fn button_from_win32(msg: u32, wparam: WPARAM) -> Option<PointerButton> {
    Some(match msg {
        WM_LBUTTONDOWN | WM_LBUTTONUP => PointerButton::Primary,
        WM_RBUTTONDOWN | WM_RBUTTONUP => PointerButton::Secondary,
        WM_MBUTTONDOWN | WM_MBUTTONUP => PointerButton::Auxiliary,
        WM_XBUTTONDOWN | WM_XBUTTONUP => match wparam >> 16 & 0xffff {
            // XBUTTON1 is defined as back, XBUTTON2 as forward.
            1 => PointerButton::X1,
            2 => PointerButton::X2,
            _ => return None,
        },
        _ => return None,
    })
}
