// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Support routines for converting pointer data from the raw Win32 API.

use dpi::PhysicalPosition;
use ui_events::pointer::{PointerButton, PointerId};
use windows_sys::Win32::Foundation::{LPARAM, POINT, WPARAM};
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

/// Extract signed coordinates from a packed mouse-message `LPARAM`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "Each coordinate is explicitly masked to its signed 16-bit Win32 field."
)]
pub(crate) fn point_from_lparam(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam & 0xffff) as i16 as i32,
        y: (lparam >> 16 & 0xffff) as i16 as i32,
    }
}

/// Extract a client-space physical position from a packed mouse-message `LPARAM`.
pub(crate) fn position_from_lparam(lparam: LPARAM) -> PhysicalPosition<f64> {
    let point = point_from_lparam(lparam);
    PhysicalPosition::new(f64::from(point.x), f64::from(point.y))
}

/// Convert a platform touch identifier without colliding with [`PointerId::PRIMARY`].
pub(crate) fn touch_pointer_id(platform_id: u32) -> Option<PointerId> {
    PointerId::new(u64::from(platform_id) + 2)
}

/// Return whether a legacy mouse message was promoted from Windows Touch.
pub(crate) const fn is_promoted_touch(extra_info: LPARAM) -> bool {
    const SIGNATURE_MASK: usize = 0xffff_ff00;
    const MI_WP_SIGNATURE: usize = 0xff51_5700;
    const MI_WP_TOUCH: usize = 0x80;

    let extra_info = extra_info as usize;
    extra_info & SIGNATURE_MASK == MI_WP_SIGNATURE && extra_info & MI_WP_TOUCH != 0
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

#[cfg(test)]
mod tests {
    use super::*;

    fn packed_lparam(x: i16, y: i16) -> LPARAM {
        let packed = u32::from(u16::from_ne_bytes(x.to_ne_bytes()))
            | (u32::from(u16::from_ne_bytes(y.to_ne_bytes())) << 16);
        LPARAM::try_from(packed).expect("packed mouse coordinates fit LPARAM")
    }

    #[test]
    fn packed_coordinates_remain_signed() {
        assert_eq!(
            position_from_lparam(packed_lparam(-320, 240)),
            PhysicalPosition::new(-320.0, 240.0)
        );
    }

    #[test]
    fn touch_identifier_zero_does_not_claim_primary_pointer() {
        let pointer_id = touch_pointer_id(0).expect("offset touch ID is nonzero");
        assert_eq!(pointer_id.get_inner().get(), 2);
        assert!(!pointer_id.is_primary_pointer());
    }

    #[test]
    fn promoted_touch_signature_does_not_filter_pen_or_mouse() {
        assert!(is_promoted_touch(0xff51_5780_u32 as LPARAM));
        assert!(!is_promoted_touch(0xff51_5700_u32 as LPARAM));
        assert!(!is_promoted_touch(0));
    }
}
