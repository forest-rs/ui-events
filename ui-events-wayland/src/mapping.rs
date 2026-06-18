// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Platform-neutral conversions from Wayland input primitives into
//! [`ui-events`] building blocks.
//!
//! Every function here operates on plain values — evdev codes, `f64`
//! coordinates, axis values, and modifier booleans — and never references
//! `wayland-client` types or performs any foreign-function calls. That keeps
//! the module compilable and unit-testable on every target, including the
//! `no_std` cross-compilation and Miri jobs, and is what keeps [`ui-events`]
//! and `dpi` used on non-Linux targets.
//!
//! The Linux-gated reducers convert `wayland-client` event streams into the
//! arguments these helpers accept.
//!
//! [`ui-events`]: https://docs.rs/ui-events/

use dpi::PhysicalPosition;
use ui_events::ScrollDelta;
use ui_events::keyboard::Modifiers;
use ui_events::pointer::{
    ContactGeometry, PointerButton, PointerId, PointerInfo, PointerOrientation, PointerType,
};

// evdev pointer button codes from `linux/input-event-codes.h`.
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
const BTN_SIDE: u32 = 0x113;
const BTN_EXTRA: u32 = 0x114;
const BTN_FORWARD: u32 = 0x115;
const BTN_BACK: u32 = 0x116;
const BTN_TASK: u32 = 0x117;

/// Map an evdev pointer button code to a [`PointerButton`].
///
/// The codes are the `BTN_*` values from `linux/input-event-codes.h` that
/// `wl_pointer` reports in its button events:
///
/// - `BTN_LEFT` → [`PointerButton::Primary`]
/// - `BTN_RIGHT` → [`PointerButton::Secondary`]
/// - `BTN_MIDDLE` → [`PointerButton::Auxiliary`]
/// - `BTN_SIDE` → [`PointerButton::X1`]
/// - `BTN_EXTRA` → [`PointerButton::X2`]
/// - `BTN_FORWARD` → [`PointerButton::B7`]
/// - `BTN_BACK` → [`PointerButton::B8`]
/// - `BTN_TASK` → [`PointerButton::B9`]
///
/// `BTN_FORWARD`, `BTN_BACK`, and `BTN_TASK` have no dedicated `ui-events`
/// button, so they map into the generic `B7`..`B9` range rather than being
/// conflated with the [`PointerButton::X1`]/[`PointerButton::X2`] side buttons.
/// Any other code returns `None`.
pub fn pointer_button_from_evdev(code: u32) -> Option<PointerButton> {
    Some(match code {
        BTN_LEFT => PointerButton::Primary,
        BTN_RIGHT => PointerButton::Secondary,
        BTN_MIDDLE => PointerButton::Auxiliary,
        BTN_SIDE => PointerButton::X1,
        BTN_EXTRA => PointerButton::X2,
        BTN_FORWARD => PointerButton::B7,
        BTN_BACK => PointerButton::B8,
        BTN_TASK => PointerButton::B9,
        _ => return None,
    })
}

/// Convert Wayland surface-local logical coordinates to physical pixels.
///
/// `wl_pointer` and `wl_touch` deliver surface-local coordinates as logical
/// (post-scale) `f64` values; the bindings already convert the on-the-wire
/// 24.8 fixed-point representation to `f64`, so no fixed-point conversion is
/// needed here. Multiplying by `scale_factor` yields the physical pixels that
/// [`PointerState`] positions use.
///
/// Non-finite coordinates are treated as `0.0`, and a non-positive or
/// non-finite `scale_factor` falls back to `1.0`.
///
/// [`PointerState`]: ui_events::pointer::PointerState
pub fn physical_position_from_logical(x: f64, y: f64, scale_factor: f64) -> PhysicalPosition<f64> {
    let scale_factor = positive_finite_or(scale_factor, 1.0);
    PhysicalPosition {
        x: finite_or(x, 0.0) * scale_factor,
        y: finite_or(y, 0.0) * scale_factor,
    }
}

/// The scroll axis source reported by `wl_pointer`.
///
/// This mirrors `wl_pointer::AxisSource` as plain values so this module needs
/// no `wayland-client` dependency; the reducer translates the protocol enum
/// into this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisSource {
    /// A physical scroll wheel with discrete detents.
    Wheel,
    /// A finger on a touchpad.
    Finger,
    /// Continuous, unbounded scrolling (for example a trackball or ring).
    Continuous,
    /// A tilting scroll wheel.
    WheelTilt,
}

/// One `wl_pointer` frame's accumulated scroll, expressed as plain values.
///
/// `wl_pointer` batches axis events between `frame` events. A reducer sums each
/// signal over a frame and then calls [`scroll_delta_from_axis_frame`] once per
/// frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AxisFrame {
    /// The axis source for this frame, if the compositor reported one.
    pub source: Option<AxisSource>,
    /// Continuous axis value in logical pixels, `(x, y)`.
    pub value: (f64, f64),
    /// High-resolution wheel steps where `120` equals one detent, `(x, y)`
    /// (`wl_pointer::axis_value120`, version 8+).
    pub value120: (i32, i32),
    /// Deprecated discrete detent count, `(x, y)` (`wl_pointer::axis_discrete`).
    pub discrete: (i32, i32),
}

/// Convert an accumulated [`AxisFrame`] into a [`ScrollDelta`].
///
/// Returns `None` when the frame carries no scroll motion.
///
/// Wheel sources (and frames with no reported source) produce a
/// [`ScrollDelta::LineDelta`] measured in wheel detents: high-resolution
/// `value120` is preferred (divided by `120`), falling back to the deprecated
/// discrete count, and finally to the continuous logical-pixel value as a
/// [`ScrollDelta::PixelDelta`] when no detent signal is present.
///
/// Finger and continuous sources produce a [`ScrollDelta::PixelDelta`] in
/// physical pixels (`value * scale_factor`).
///
/// Wayland's positive axis values mean down/right, matching [`ScrollDelta`]'s
/// Y-down convention, so signs are preserved.
pub fn scroll_delta_from_axis_frame(frame: AxisFrame, scale_factor: f64) -> Option<ScrollDelta> {
    let scale_factor = positive_finite_or(scale_factor, 1.0);
    let pixel_delta = |(x, y): (f64, f64)| {
        ScrollDelta::PixelDelta(PhysicalPosition {
            x: finite_or(x, 0.0) * scale_factor,
            y: finite_or(y, 0.0) * scale_factor,
        })
    };
    match frame.source {
        Some(AxisSource::Finger | AxisSource::Continuous) => {
            let (x, y) = frame.value;
            (x != 0.0 || y != 0.0).then(|| pixel_delta((x, y)))
        }
        // `Wheel`, `WheelTilt`, or an unreported source: prefer discrete detents.
        _ => {
            let (x120, y120) = frame.value120;
            let (xd, yd) = frame.discrete;
            if x120 != 0 || y120 != 0 {
                Some(ScrollDelta::LineDelta(
                    x120 as f32 / 120.0,
                    y120 as f32 / 120.0,
                ))
            } else if xd != 0 || yd != 0 {
                Some(ScrollDelta::LineDelta(xd as f32, yd as f32))
            } else {
                let (x, y) = frame.value;
                (x != 0.0 || y != 0.0).then(|| pixel_delta((x, y)))
            }
        }
    }
}

/// Offset added to a platform-provided pointer identifier before constructing a
/// [`PointerId`], so it can never collide with the reserved
/// [`PointerId::PRIMARY`] (whose underlying value is `1`).
///
/// Offsetting platform id `0` by `2` makes the first platform-derived id `2`.
pub const POINTER_ID_OFFSET: u64 = 2;

/// Build a non-primary [`PointerId`] from a platform identifier.
///
/// Applies [`POINTER_ID_OFFSET`] so the result never aliases
/// [`PointerId::PRIMARY`]. Returns `None` only on arithmetic overflow.
pub fn pointer_id_from_platform_id(id: u64) -> Option<PointerId> {
    id.checked_add(POINTER_ID_OFFSET).and_then(PointerId::new)
}

/// Build a [`PointerInfo`] for the primary pointer of the given [`PointerType`].
pub fn primary_pointer_info(pointer_type: PointerType) -> PointerInfo {
    PointerInfo {
        pointer_id: Some(PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type,
    }
}

/// Build a [`PointerInfo`] for a non-primary pointer of the given
/// [`PointerType`] from a platform identifier.
///
/// The identifier is offset through [`pointer_id_from_platform_id`] to avoid
/// colliding with [`PointerId::PRIMARY`].
pub fn pointer_info_from_platform_id(pointer_type: PointerType, id: u64) -> PointerInfo {
    PointerInfo {
        pointer_id: pointer_id_from_platform_id(id),
        persistent_device_id: None,
        pointer_type,
    }
}

/// Build a [`Modifiers`] set from individual modifier booleans.
///
/// Without an xkb keymap, `wl_keyboard` modifier state is an opaque bitmask, so
/// the keyboard reducer tracks these booleans from physical modifier key
/// presses and assembles them here.
pub fn modifiers_from_bools(ctrl: bool, alt: bool, shift: bool, meta: bool) -> Modifiers {
    let mut m = Modifiers::default();
    if ctrl {
        m.insert(Modifiers::CONTROL);
    }
    if alt {
        m.insert(Modifiers::ALT);
    }
    if shift {
        m.insert(Modifiers::SHIFT);
    }
    if meta {
        m.insert(Modifiers::META);
    }
    m
}

/// Build a [`PointerInfo`] for a touch contact, reserving [`PointerId::PRIMARY`]
/// for the primary contact.
///
/// `wl_touch` identifies each contact with a small non-negative integer that is
/// unique among the contacts currently down. The contact whose id equals
/// `primary_id` — conventionally the lowest active id, mirroring the primary
/// pointer in the web backend — is mapped to [`PointerId::PRIMARY`]. Every other
/// contact is offset through [`pointer_info_from_platform_id`] so it cannot
/// collide with the reserved primary id.
pub fn touch_pointer_info(id: u64, primary_id: u64) -> PointerInfo {
    if id == primary_id {
        primary_pointer_info(PointerType::Touch)
    } else {
        pointer_info_from_platform_id(PointerType::Touch, id)
    }
}

/// Convert a `wl_touch` contact ellipse into a [`ContactGeometry`].
///
/// `wl_touch::shape` reports the lengths of the major and minor axes of the
/// ellipse approximating the contact, in surface-local (logical) coordinates.
/// Wayland aligns the major axis with the surface y-axis at the zero
/// orientation, so the major-axis length becomes the geometry's `height` and the
/// minor-axis length its `width`; the major-axis direction is reported
/// separately as the azimuth of [`touch_orientation_from_degrees`]. Both lengths
/// are multiplied by `scale_factor` to produce physical pixels.
///
/// A non-positive or non-finite axis length or `scale_factor` falls back to a
/// single physical pixel, matching the [`ContactGeometry`] default.
pub fn contact_geometry_from_shape(major: f64, minor: f64, scale_factor: f64) -> ContactGeometry {
    let scale_factor = positive_finite_or(scale_factor, 1.0);
    ContactGeometry {
        width: positive_finite_or(minor * scale_factor, 1.0),
        height: positive_finite_or(major * scale_factor, 1.0),
    }
}

/// Convert a `wl_touch` contact orientation into a [`PointerOrientation`].
///
/// `wl_touch::orientation` reports the clockwise angle, in degrees, between the
/// contact ellipse's major axis and the positive surface y-axis. A touch contact
/// lies flat against the surface, so the altitude is always perpendicular
/// (`π/2`); the angle is folded into the azimuth, which is `0` along the positive
/// x-axis and `π/2` along the positive y-axis. An orientation of `0°` therefore
/// yields the default [`PointerOrientation`].
///
/// A non-finite angle falls back to the default orientation.
pub fn touch_orientation_from_degrees(orientation_deg: f64) -> PointerOrientation {
    if !orientation_deg.is_finite() {
        return PointerOrientation::default();
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Wayland reports orientation as f64 degrees; ui-events stores azimuth as f32"
    )]
    let azimuth = (core::f32::consts::FRAC_PI_2 + (orientation_deg as f32).to_radians())
        .rem_euclid(core::f32::consts::TAU);
    PointerOrientation {
        altitude: core::f32::consts::FRAC_PI_2,
        azimuth,
    }
}

/// Convert a `zwp_pointer_gesture_pinch_v1` absolute pinch scale into the
/// signed, per-update scale fraction carried by [`PointerGesture::Pinch`].
///
/// Wayland reports the pinch `scale` as an absolute ratio relative to the
/// gesture's `begin` event, which is implicitly `1.0` — a `scale` of `2.0` means
/// the fingers are twice as far apart as they were at `begin`.
/// [`PointerGesture::Pinch`] instead carries the signed fractional change for a
/// single update, where `0.1` means "grow by 10%" and the update rule is
/// `new_scale = current_scale * (1.0 + delta)`. Given the absolute scale from
/// the previous update (or `1.0` at the start of the gesture) as `previous` and
/// the absolute scale from the current update as `current`, this returns
/// `current / previous - 1.0`.
///
/// A non-finite or non-positive `previous` falls back to `1.0`, and a
/// non-finite or non-positive `current` is treated as unchanged from `previous`,
/// yielding `0.0`.
///
/// [`PointerGesture::Pinch`]: ui_events::pointer::PointerGesture::Pinch
#[expect(
    clippy::cast_possible_truncation,
    reason = "Wayland reports scale as f64; ui-events stores the pinch fraction as f32"
)]
pub fn pinch_scale_fraction(previous: f64, current: f64) -> f32 {
    let previous = positive_finite_or(previous, 1.0);
    let current = positive_finite_or(current, previous);
    (current / previous - 1.0) as f32
}

/// Convert a `zwp_pointer_gesture_pinch_v1` rotation into the clockwise radians
/// carried by [`PointerGesture::Rotate`].
///
/// Wayland reports the pinch `rotation` as an angle in degrees, measured
/// clockwise, accumulated since the previous `begin` or `update` event — already
/// a per-update delta. [`PointerGesture::Rotate`] is likewise a clockwise,
/// per-update delta, but measured in radians, so this is a plain
/// degrees-to-radians conversion that preserves the sign. A non-finite angle
/// yields `0.0`.
///
/// [`PointerGesture::Rotate`]: ui_events::pointer::PointerGesture::Rotate
#[expect(
    clippy::cast_possible_truncation,
    reason = "Wayland reports rotation as f64 degrees; ui-events stores the angle as f32"
)]
pub fn rotation_radians_from_degrees(degrees: f64) -> f32 {
    finite_or(degrees, 0.0).to_radians() as f32
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evdev_buttons_map_to_expected_pointer_buttons() {
        assert_eq!(
            pointer_button_from_evdev(BTN_LEFT),
            Some(PointerButton::Primary)
        );
        assert_eq!(
            pointer_button_from_evdev(BTN_RIGHT),
            Some(PointerButton::Secondary)
        );
        assert_eq!(
            pointer_button_from_evdev(BTN_MIDDLE),
            Some(PointerButton::Auxiliary)
        );
        assert_eq!(pointer_button_from_evdev(BTN_SIDE), Some(PointerButton::X1));
        assert_eq!(
            pointer_button_from_evdev(BTN_EXTRA),
            Some(PointerButton::X2)
        );
        assert_eq!(
            pointer_button_from_evdev(BTN_FORWARD),
            Some(PointerButton::B7)
        );
        assert_eq!(pointer_button_from_evdev(BTN_BACK), Some(PointerButton::B8));
        assert_eq!(pointer_button_from_evdev(BTN_TASK), Some(PointerButton::B9));
    }

    #[test]
    fn evdev_unknown_button_is_none() {
        // Just below `BTN_LEFT`, just above `BTN_TASK`, and `BTN_TOUCH`.
        assert_eq!(pointer_button_from_evdev(0x10f), None);
        assert_eq!(pointer_button_from_evdev(0x118), None);
        assert_eq!(pointer_button_from_evdev(0x14a), None);
    }

    #[test]
    fn logical_coordinates_scale_to_physical_pixels() {
        assert_eq!(
            physical_position_from_logical(10.0, 20.0, 2.0),
            PhysicalPosition { x: 20.0, y: 40.0 }
        );
    }

    #[test]
    fn position_sanitizes_non_finite_inputs() {
        // Non-finite scale falls back to 1.0; non-finite coordinate to 0.0.
        assert_eq!(
            physical_position_from_logical(f64::NAN, 5.0, f64::INFINITY),
            PhysicalPosition { x: 0.0, y: 5.0 }
        );
    }

    #[test]
    fn wheel_frame_prefers_value120_over_discrete() {
        let frame = AxisFrame {
            source: Some(AxisSource::Wheel),
            value: (0.0, 30.0),
            value120: (0, 240),
            discrete: (0, 2),
        };
        assert_eq!(
            scroll_delta_from_axis_frame(frame, 2.0),
            Some(ScrollDelta::LineDelta(0.0, 2.0))
        );
    }

    #[test]
    fn wheel_frame_falls_back_to_discrete() {
        let frame = AxisFrame {
            source: Some(AxisSource::Wheel),
            discrete: (0, -1),
            ..Default::default()
        };
        assert_eq!(
            scroll_delta_from_axis_frame(frame, 1.0),
            Some(ScrollDelta::LineDelta(0.0, -1.0))
        );
    }

    #[test]
    fn finger_frame_is_pixel_delta_scaled() {
        let frame = AxisFrame {
            source: Some(AxisSource::Finger),
            value: (3.0, -4.0),
            ..Default::default()
        };
        assert_eq!(
            scroll_delta_from_axis_frame(frame, 2.0),
            Some(ScrollDelta::PixelDelta(PhysicalPosition {
                x: 6.0,
                y: -8.0
            }))
        );
    }

    #[test]
    fn empty_frame_is_none() {
        assert_eq!(
            scroll_delta_from_axis_frame(AxisFrame::default(), 1.0),
            None
        );
    }

    #[test]
    fn sourceless_frame_with_only_value_is_pixel_delta() {
        let frame = AxisFrame {
            source: None,
            value: (0.0, 12.0),
            ..Default::default()
        };
        assert_eq!(
            scroll_delta_from_axis_frame(frame, 1.0),
            Some(ScrollDelta::PixelDelta(PhysicalPosition {
                x: 0.0,
                y: 12.0
            }))
        );
    }

    #[test]
    fn platform_pointer_id_does_not_collide_with_primary() {
        let id = pointer_id_from_platform_id(0).expect("offset id should be valid");
        assert_eq!(id.get_inner().get(), POINTER_ID_OFFSET);
        assert_ne!(id, PointerId::PRIMARY);
    }

    #[test]
    fn primary_pointer_info_is_primary() {
        let info = primary_pointer_info(PointerType::Mouse);
        assert!(info.is_primary_pointer());
        assert_eq!(info.pointer_type, PointerType::Mouse);
    }

    #[test]
    fn platform_pointer_info_is_not_primary() {
        let info = pointer_info_from_platform_id(PointerType::Touch, 0);
        assert!(!info.is_primary_pointer());
        assert_eq!(info.pointer_type, PointerType::Touch);
        assert_eq!(info.pointer_id, PointerId::new(POINTER_ID_OFFSET));
    }

    #[test]
    fn modifiers_from_bools_sets_expected_bits() {
        let mods = modifiers_from_bools(true, false, true, false);
        assert!(mods.ctrl());
        assert!(!mods.alt());
        assert!(mods.shift());
        assert!(!mods.meta());
    }

    #[test]
    fn touch_primary_contact_is_primary_pointer() {
        let info = touch_pointer_info(3, 3);
        assert!(info.is_primary_pointer());
        assert_eq!(info.pointer_type, PointerType::Touch);
    }

    #[test]
    fn touch_non_primary_contact_is_offset() {
        let info = touch_pointer_info(3, 1);
        assert!(!info.is_primary_pointer());
        assert_eq!(info.pointer_type, PointerType::Touch);
        assert_eq!(info.pointer_id, PointerId::new(3 + POINTER_ID_OFFSET));
    }

    #[test]
    fn shape_maps_minor_to_width_and_major_to_height_scaled() {
        let geometry = contact_geometry_from_shape(10.0, 4.0, 2.0);
        assert_eq!(geometry.width, 8.0);
        assert_eq!(geometry.height, 20.0);
    }

    #[test]
    fn shape_sanitizes_degenerate_axes() {
        let geometry = contact_geometry_from_shape(0.0, f64::NAN, 1.0);
        assert_eq!(geometry.width, 1.0);
        assert_eq!(geometry.height, 1.0);
    }

    #[test]
    fn orientation_zero_degrees_is_default() {
        let orientation = touch_orientation_from_degrees(0.0);
        assert_eq!(orientation.altitude, core::f32::consts::FRAC_PI_2);
        assert!((orientation.azimuth - core::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn orientation_rotates_azimuth_clockwise_from_y_axis() {
        // 90° clockwise from the +y axis puts the major axis along azimuth π.
        let orientation = touch_orientation_from_degrees(90.0);
        assert!((orientation.azimuth - core::f32::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn orientation_non_finite_is_default() {
        assert_eq!(
            touch_orientation_from_degrees(f64::INFINITY),
            PointerOrientation::default()
        );
    }

    #[test]
    fn pinch_growth_is_a_positive_fraction() {
        // Growing from the 1.0 begin baseline to 1.1 is a 10% increase.
        assert!((pinch_scale_fraction(1.0, 1.1) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn pinch_shrink_is_a_negative_fraction() {
        assert!((pinch_scale_fraction(1.0, 0.5) + 0.5).abs() < 1e-6);
    }

    #[test]
    fn pinch_fraction_is_relative_to_the_previous_scale() {
        // Two updates each growing the absolute scale by 10% each report ~10%,
        // because the protocol's scale is absolute, not per-update.
        assert!((pinch_scale_fraction(1.1, 1.21) - 0.1).abs() < 1e-5);
    }

    #[test]
    fn pinch_fraction_sanitizes_degenerate_scales() {
        // Non-positive previous falls back to 1.0; non-finite current is treated
        // as no change.
        assert_eq!(pinch_scale_fraction(0.0, 1.0), 0.0);
        assert_eq!(pinch_scale_fraction(1.0, f64::NAN), 0.0);
    }

    #[test]
    fn rotation_converts_degrees_to_clockwise_radians() {
        assert!((rotation_radians_from_degrees(90.0) - core::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((rotation_radians_from_degrees(-45.0) + core::f32::consts::FRAC_PI_4).abs() < 1e-6);
    }

    #[test]
    fn rotation_non_finite_is_zero() {
        assert_eq!(rotation_radians_from_degrees(f64::INFINITY), 0.0);
    }
}
