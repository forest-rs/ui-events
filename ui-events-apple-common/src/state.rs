// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use dpi::{PhysicalPosition, PhysicalSize};
use ui_events::ScrollDelta;
use ui_events::pointer::PointerState;

use crate::{buttons_from_bitmask, modifiers_from_bools};

/// Build a [`PointerState`] from raw values.
///
/// Positions in points are converted to physical pixels by `scale_factor`.
/// `timestamp_secs` is the platform monotonic timebase in fractional seconds
/// since boot, not Unix epoch time.
/// Non-finite scalar inputs are sanitized to conservative defaults before
/// constructing the event state.
pub fn pointer_state_from_raw(
    x_points: f64,
    y_points: f64,
    scale_factor: f64,
    buttons_mask: u64,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
    pressure: Option<f64>,
    tangential_pressure: Option<f64>,
    contact_w_points: Option<f64>,
    contact_h_points: Option<f64>,
    timestamp_secs: f64,
    click_count: i64,
) -> PointerState {
    let scale_factor = positive_finite_or(scale_factor, 1.0);
    let x_points = finite_or(x_points, 0.0);
    let y_points = finite_or(y_points, 0.0);
    let position = PhysicalPosition {
        x: x_points * scale_factor,
        y: y_points * scale_factor,
    };
    let buttons = buttons_from_bitmask(buttons_mask);
    let default_pressure = if buttons.is_empty() { 0.0 } else { 0.5 };
    let pressure = f64_to_f32(finite_option_or(pressure, default_pressure));
    let tangential_pressure = f64_to_f32(finite_option_or(tangential_pressure, 0.0));
    let contact_geometry = PhysicalSize {
        width: non_negative_finite_option_or(contact_w_points, 1.0) * scale_factor,
        height: non_negative_finite_option_or(contact_h_points, 1.0) * scale_factor,
    };
    PointerState {
        time: secs_to_ns_u64(timestamp_secs),
        position,
        buttons,
        modifiers: modifiers_from_bools(ctrl, alt, shift, meta),
        count: (click_count.clamp(0, 255)) as u8,
        contact_geometry,
        orientation: Default::default(),
        pressure,
        tangential_pressure,
        scale_factor,
    }
}

/// Build a [`ScrollDelta`] from deltas in points.
///
/// When `precise` is true, deltas are interpreted as pixel deltas and mapped
/// to [`ScrollDelta::PixelDelta`]. Otherwise they are interpreted as line
/// deltas and mapped to [`ScrollDelta::LineDelta`]. Delta signs are preserved.
pub fn pointer_scroll_delta_from_raw(
    scale_factor: f64,
    precise: bool,
    delta_x_points: f64,
    delta_y_points: f64,
) -> ScrollDelta {
    let scale_factor = positive_finite_or(scale_factor, 1.0);
    let delta_x_points = finite_or(delta_x_points, 0.0);
    let delta_y_points = finite_or(delta_y_points, 0.0);
    if precise {
        ScrollDelta::PixelDelta(PhysicalPosition {
            x: delta_x_points * scale_factor,
            y: delta_y_points * scale_factor,
        })
    } else {
        ScrollDelta::LineDelta(f64_to_f32(delta_x_points), f64_to_f32(delta_y_points))
    }
}

#[inline]
#[expect(
    clippy::cast_possible_truncation,
    reason = "Converting fractional seconds (f64) to integer ns intentionally"
)]
fn secs_to_ns_u64(secs: f64) -> u64 {
    let nanos = finite_or(secs, 0.0) * 1_000_000_000.0;
    if nanos <= 0.0 {
        0
    } else if nanos >= u64::MAX as f64 {
        u64::MAX
    } else {
        nanos as u64
    }
}

#[inline]
#[expect(
    clippy::cast_possible_truncation,
    reason = "Platform deltas/pressure are f64; ui-events stores f32"
)]
fn f64_to_f32(v: f64) -> f32 {
    v as f32
}

#[inline]
fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

#[inline]
fn positive_finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

#[inline]
fn finite_option_or(value: Option<f64>, fallback: f64) -> f64 {
    value.filter(|value| value.is_finite()).unwrap_or(fallback)
}

#[inline]
fn non_negative_finite_option_or(value: Option<f64>, fallback: f64) -> f64 {
    value
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_state_from_raw_sanitizes_non_finite_scalars() {
        let state = pointer_state_from_raw(
            f64::NAN,
            f64::INFINITY,
            f64::INFINITY,
            1,
            false,
            false,
            false,
            false,
            Some(f64::NAN),
            Some(f64::NEG_INFINITY),
            Some(f64::NAN),
            Some(-1.0),
            f64::NAN,
            1,
        );

        assert_eq!(state.position.x, 0.0);
        assert_eq!(state.position.y, 0.0);
        assert_eq!(state.scale_factor, 1.0);
        assert_eq!(state.pressure, 0.5);
        assert_eq!(state.tangential_pressure, 0.0);
        assert_eq!(state.contact_geometry.width, 1.0);
        assert_eq!(state.contact_geometry.height, 1.0);
        assert_eq!(state.time, 0);
    }

    #[test]
    fn pointer_scroll_delta_from_raw_sanitizes_non_finite_deltas() {
        let delta = pointer_scroll_delta_from_raw(2.0, true, f64::NAN, f64::INFINITY);

        assert_eq!(
            delta,
            ScrollDelta::PixelDelta(PhysicalPosition { x: 0.0, y: 0.0 })
        );
    }

    #[test]
    fn pointer_scroll_delta_from_raw_preserves_precise_delta_signs() {
        let delta = pointer_scroll_delta_from_raw(2.0, true, 3.0, -4.0);

        assert_eq!(
            delta,
            ScrollDelta::PixelDelta(PhysicalPosition { x: 6.0, y: -8.0 })
        );
    }

    #[test]
    fn pointer_scroll_delta_from_raw_preserves_line_delta_signs() {
        let delta = pointer_scroll_delta_from_raw(2.0, false, 3.0, -4.0);

        assert_eq!(delta, ScrollDelta::LineDelta(3.0, -4.0));
    }
}
