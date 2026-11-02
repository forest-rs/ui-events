// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Mouse wheel preferences and viewport navigation.

use ui_events::ScrollDelta;
use windows_sys::Win32::UI::WindowsAndMessaging::SystemParametersInfoW;

#[derive(Clone, Copy, Debug)]
pub(super) enum WheelAxis {
    Horizontal,
    Vertical,
}

pub(super) fn wheel_delta(axis: WheelAxis, notches: f32, preference: u32) -> Option<ScrollDelta> {
    if notches == 0.0 || preference == 0 {
        return None;
    }

    // Win32 vertical wheel rotation is content motion (positive moves content down), while
    // ScrollDelta is viewport navigation. Horizontal wheel rotation already has navigation sign.
    let notches = match axis {
        WheelAxis::Horizontal => notches,
        WheelAxis::Vertical => -notches,
    };
    let (x, y) = match axis {
        WheelAxis::Horizontal => (notches, 0.0),
        WheelAxis::Vertical => (0.0, notches),
    };

    if preference == u32::MAX {
        Some(ScrollDelta::PageDelta(x, y))
    } else {
        Some(ScrollDelta::LineDelta(
            x * preference as f32,
            y * preference as f32,
        ))
    }
}

pub(super) fn wheel_preference(param: u32) -> u32 {
    /// The default number of lines/characters scrolled per notch of a vertical
    /// or horizontal mouse wheel.
    const DEFAULT: u32 = 3;

    let mut preference = DEFAULT;
    // SAFETY: `preference` is a valid pointer to the `UINT` output required by both supported
    // `SPI_GETWHEELSCROLL*` parameters.
    if unsafe { SystemParametersInfoW(param, 0, std::ptr::from_mut(&mut preference).cast(), 0) }
        == 0
    {
        DEFAULT
    } else {
        preference
    }
}
