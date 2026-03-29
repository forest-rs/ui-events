// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UIKit-specific mapping helpers for building `ui-events` from raw values.

use dpi::PhysicalSize;
use ui_events::keyboard::{Code, Location, Modifiers, NamedKey};
use ui_events::pointer::{PointerButton, PointerInfo, PointerType};
pub use ui_events_apple_common::{
    buttons_from_bitmask, modifiers_from_bools, pointer_info_primary_for_type,
    pointer_state_from_raw, try_from_button_index,
};

#[cfg(feature = "std")]
fn sine(angle: f64) -> f64 {
    angle.sin()
}

#[cfg(all(not(feature = "std"), feature = "libm"))]
fn sine(angle: f64) -> f64 {
    libm::sin(angle)
}

#[cfg(all(not(feature = "std"), not(feature = "libm")))]
compile_error!("ui-events-uikit requires either the `std` or `libm` feature");

#[allow(
    dead_code,
    reason = "Used on iOS/tvOS targets and by unit tests for UIKit contact geometry"
)]
fn contact_geometry_from_points(
    width_points: Option<f64>,
    height_points: Option<f64>,
    scale_factor: f64,
) -> PhysicalSize<f64> {
    PhysicalSize {
        width: width_points.unwrap_or(1.0) * scale_factor,
        height: height_points.unwrap_or(1.0) * scale_factor,
    }
}

#[allow(
    dead_code,
    reason = "Used on iOS/tvOS targets where the UIKit adapter is compiled"
)]
pub(crate) fn pointer_info_from_platform_pointer_id(
    pointer_type: PointerType,
    pointer_id: u64,
) -> PointerInfo {
    ui_events_apple_common::pointer_info_from_platform_pointer_id(pointer_type, pointer_id)
}

#[allow(
    dead_code,
    reason = "Used on iOS/tvOS targets where the UIKit adapter is compiled"
)]
pub(crate) fn contact_buttons_mask_for_uikit_pointer(
    pointer_type: PointerType,
    active: bool,
    event_buttons_mask: u64,
) -> u64 {
    let mut buttons_mask = event_buttons_mask;
    if active && matches!(pointer_type, PointerType::Pen) {
        buttons_mask |= PointerButton::Primary as u64;
    }
    buttons_mask
}

#[allow(
    dead_code,
    reason = "Used on iOS/tvOS targets where the UIKit adapter is compiled"
)]
pub(crate) fn contact_button_for_uikit_pointer(pointer_type: PointerType) -> Option<PointerButton> {
    matches!(pointer_type, PointerType::Pen).then_some(PointerButton::Primary)
}

#[allow(
    dead_code,
    reason = "Used on iOS/tvOS targets where the UIKit adapter is compiled"
)]
mod hid {
    pub(super) const A: u16 = 0x04;
    pub(super) const Z: u16 = 0x1D;
    pub(super) const DIGIT_1: u16 = 0x1E; // through 0x27 => 0..9
    pub(super) const DIGIT_0: u16 = 0x27;
    pub(super) const RETURN: u16 = 0x28;
    pub(super) const ESCAPE: u16 = 0x29;
    pub(super) const BACKSPACE: u16 = 0x2A; // Delete (Backspace)
    pub(super) const TAB: u16 = 0x2B;
    pub(super) const SPACE: u16 = 0x2C;
    pub(super) const MINUS: u16 = 0x2D;
    pub(super) const EQUAL: u16 = 0x2E;
    pub(super) const BRACKET_LEFT: u16 = 0x2F;
    pub(super) const BRACKET_RIGHT: u16 = 0x30;
    pub(super) const BACKSLASH: u16 = 0x31;
    pub(super) const SEMICOLON: u16 = 0x33;
    pub(super) const QUOTE: u16 = 0x34;
    pub(super) const GRAVE: u16 = 0x35;
    pub(super) const COMMA: u16 = 0x36;
    pub(super) const PERIOD: u16 = 0x37;
    pub(super) const SLASH: u16 = 0x38;
    pub(super) const CAPS_LOCK: u16 = 0x39;
    pub(super) const F1: u16 = 0x3A; // through F12: 0x45
    pub(super) const F12: u16 = 0x45;
    pub(super) const F13: u16 = 0x68; // through F24: 0x73
    pub(super) const F24: u16 = 0x73;
    pub(super) const PRINT_SCREEN: u16 = 0x46;
    pub(super) const SCROLL_LOCK: u16 = 0x47;
    pub(super) const PAUSE: u16 = 0x48;
    pub(super) const INSERT: u16 = 0x49;
    pub(super) const HOME: u16 = 0x4A;
    pub(super) const PAGE_UP: u16 = 0x4B;
    pub(super) const DELETE_FORWARD: u16 = 0x4C;
    pub(super) const END: u16 = 0x4D;
    pub(super) const PAGE_DOWN: u16 = 0x4E;
    pub(super) const RIGHT_ARROW: u16 = 0x4F;
    pub(super) const LEFT_ARROW: u16 = 0x50;
    pub(super) const DOWN_ARROW: u16 = 0x51;
    pub(super) const UP_ARROW: u16 = 0x52;
    pub(super) const NUM_LOCK: u16 = 0x53;
    pub(super) const NUMPAD_DIVIDE: u16 = 0x54;
    pub(super) const NUMPAD_MULTIPLY: u16 = 0x55;
    pub(super) const NUMPAD_SUBTRACT: u16 = 0x56;
    pub(super) const NUMPAD_ADD: u16 = 0x57;
    pub(super) const NUMPAD_ENTER: u16 = 0x58;
    pub(super) const NUMPAD_1: u16 = 0x59; // ..0x62 = 1..0
    pub(super) const NUMPAD_0: u16 = 0x62;
    pub(super) const NUMPAD_DECIMAL: u16 = 0x63;
    pub(super) const NUMPAD_EQUAL: u16 = 0x67;
    pub(super) const CONTROL_LEFT: u16 = 0xE0;
    pub(super) const SHIFT_LEFT: u16 = 0xE1;
    pub(super) const ALT_LEFT: u16 = 0xE2;
    pub(super) const META_LEFT: u16 = 0xE3;
    pub(super) const CONTROL_RIGHT: u16 = 0xE4;
    pub(super) const SHIFT_RIGHT: u16 = 0xE5;
    pub(super) const ALT_RIGHT: u16 = 0xE6;
    pub(super) const META_RIGHT: u16 = 0xE7;
}

#[allow(
    dead_code,
    reason = "Used on iOS/tvOS targets where the UIKit adapter is compiled"
)]
const UIKIT_SHIFT_BIT: u64 = 1 << 17;
#[allow(
    dead_code,
    reason = "Used on iOS/tvOS targets where the UIKit adapter is compiled"
)]
const UIKIT_CONTROL_BIT: u64 = 1 << 18;
#[allow(
    dead_code,
    reason = "Used on iOS/tvOS targets where the UIKit adapter is compiled"
)]
const UIKIT_ALT_BIT: u64 = 1 << 19;
#[allow(
    dead_code,
    reason = "Used on iOS/tvOS targets where the UIKit adapter is compiled"
)]
const UIKIT_META_BIT: u64 = 1 << 20;

#[allow(
    dead_code,
    reason = "Used on iOS/tvOS targets where the UIKit adapter is compiled"
)]
pub(crate) fn modifiers_from_uikit_modifier_bits(bits: u64) -> Modifiers {
    modifiers_from_bools(
        (bits & UIKIT_CONTROL_BIT) != 0,
        (bits & UIKIT_ALT_BIT) != 0,
        (bits & UIKIT_SHIFT_BIT) != 0,
        (bits & UIKIT_META_BIT) != 0,
    )
}

#[allow(
    dead_code,
    reason = "Used on iOS/tvOS targets where the UIKit adapter is compiled"
)]
pub(crate) fn hid_usage_to_code_named_location(usage: u16) -> (Code, Option<NamedKey>, Location) {
    use hid as H;
    if (H::A..=H::Z).contains(&usage) {
        let code = match usage - H::A {
            0 => Code::KeyA,
            1 => Code::KeyB,
            2 => Code::KeyC,
            3 => Code::KeyD,
            4 => Code::KeyE,
            5 => Code::KeyF,
            6 => Code::KeyG,
            7 => Code::KeyH,
            8 => Code::KeyI,
            9 => Code::KeyJ,
            10 => Code::KeyK,
            11 => Code::KeyL,
            12 => Code::KeyM,
            13 => Code::KeyN,
            14 => Code::KeyO,
            15 => Code::KeyP,
            16 => Code::KeyQ,
            17 => Code::KeyR,
            18 => Code::KeyS,
            19 => Code::KeyT,
            20 => Code::KeyU,
            21 => Code::KeyV,
            22 => Code::KeyW,
            23 => Code::KeyX,
            24 => Code::KeyY,
            _ => Code::KeyZ,
        };
        return (code, None, Location::Standard);
    }

    if (H::DIGIT_1..=H::DIGIT_0).contains(&usage) {
        let code = match usage {
            0x1E => Code::Digit1,
            0x1F => Code::Digit2,
            0x20 => Code::Digit3,
            0x21 => Code::Digit4,
            0x22 => Code::Digit5,
            0x23 => Code::Digit6,
            0x24 => Code::Digit7,
            0x25 => Code::Digit8,
            0x26 => Code::Digit9,
            _ => Code::Digit0,
        };
        return (code, None, Location::Standard);
    }

    if (H::NUMPAD_1..=H::NUMPAD_0).contains(&usage) {
        let code = match usage {
            0x59 => Code::Numpad1,
            0x5A => Code::Numpad2,
            0x5B => Code::Numpad3,
            0x5C => Code::Numpad4,
            0x5D => Code::Numpad5,
            0x5E => Code::Numpad6,
            0x5F => Code::Numpad7,
            0x60 => Code::Numpad8,
            0x61 => Code::Numpad9,
            _ => Code::Numpad0,
        };
        return (code, None, Location::Numpad);
    }

    match usage {
        H::RETURN => (Code::Enter, Some(NamedKey::Enter), Location::Standard),
        H::ESCAPE => (Code::Escape, Some(NamedKey::Escape), Location::Standard),
        H::BACKSPACE => (
            Code::Backspace,
            Some(NamedKey::Backspace),
            Location::Standard,
        ),
        H::TAB => (Code::Tab, Some(NamedKey::Tab), Location::Standard),
        H::SPACE => (Code::Space, None, Location::Standard),
        H::MINUS => (Code::Minus, None, Location::Standard),
        H::EQUAL => (Code::Equal, None, Location::Standard),
        H::BRACKET_LEFT => (Code::BracketLeft, None, Location::Standard),
        H::BRACKET_RIGHT => (Code::BracketRight, None, Location::Standard),
        H::BACKSLASH => (Code::Backslash, None, Location::Standard),
        H::SEMICOLON => (Code::Semicolon, None, Location::Standard),
        H::QUOTE => (Code::Quote, None, Location::Standard),
        H::GRAVE => (Code::Backquote, None, Location::Standard),
        H::COMMA => (Code::Comma, None, Location::Standard),
        H::PERIOD => (Code::Period, None, Location::Standard),
        H::SLASH => (Code::Slash, None, Location::Standard),
        H::CAPS_LOCK => (Code::CapsLock, Some(NamedKey::CapsLock), Location::Standard),
        H::F1..=H::F12 => {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "The F-key range is bounded to 1..=12 before narrowing to u8"
            )]
            let idx = (usage - H::F1) as u8 + 1;
            let (code, named) = match idx {
                1 => (Code::F1, NamedKey::F1),
                2 => (Code::F2, NamedKey::F2),
                3 => (Code::F3, NamedKey::F3),
                4 => (Code::F4, NamedKey::F4),
                5 => (Code::F5, NamedKey::F5),
                6 => (Code::F6, NamedKey::F6),
                7 => (Code::F7, NamedKey::F7),
                8 => (Code::F8, NamedKey::F8),
                9 => (Code::F9, NamedKey::F9),
                10 => (Code::F10, NamedKey::F10),
                11 => (Code::F11, NamedKey::F11),
                _ => (Code::F12, NamedKey::F12),
            };
            (code, Some(named), Location::Standard)
        }
        H::F13..=H::F24 => {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "The F-key range is bounded to 13..=24 before narrowing to u8"
            )]
            let idx = (usage - H::F13) as u8 + 13;
            let (code, named) = match idx {
                13 => (Code::F13, NamedKey::F13),
                14 => (Code::F14, NamedKey::F14),
                15 => (Code::F15, NamedKey::F15),
                16 => (Code::F16, NamedKey::F16),
                17 => (Code::F17, NamedKey::F17),
                18 => (Code::F18, NamedKey::F18),
                19 => (Code::F19, NamedKey::F19),
                20 => (Code::F20, NamedKey::F20),
                21 => (Code::F21, NamedKey::F21),
                22 => (Code::F22, NamedKey::F22),
                23 => (Code::F23, NamedKey::F23),
                _ => (Code::F24, NamedKey::F24),
            };
            (code, Some(named), Location::Standard)
        }
        H::PRINT_SCREEN => (
            Code::PrintScreen,
            Some(NamedKey::PrintScreen),
            Location::Standard,
        ),
        H::SCROLL_LOCK => (
            Code::ScrollLock,
            Some(NamedKey::ScrollLock),
            Location::Standard,
        ),
        H::PAUSE => (Code::Pause, Some(NamedKey::Pause), Location::Standard),
        H::INSERT => (Code::Insert, Some(NamedKey::Insert), Location::Standard),
        H::HOME => (Code::Home, Some(NamedKey::Home), Location::Standard),
        H::PAGE_UP => (Code::PageUp, Some(NamedKey::PageUp), Location::Standard),
        H::DELETE_FORWARD => (Code::Delete, Some(NamedKey::Delete), Location::Standard),
        H::END => (Code::End, Some(NamedKey::End), Location::Standard),
        H::PAGE_DOWN => (Code::PageDown, Some(NamedKey::PageDown), Location::Standard),
        H::RIGHT_ARROW => (
            Code::ArrowRight,
            Some(NamedKey::ArrowRight),
            Location::Standard,
        ),
        H::LEFT_ARROW => (
            Code::ArrowLeft,
            Some(NamedKey::ArrowLeft),
            Location::Standard,
        ),
        H::DOWN_ARROW => (
            Code::ArrowDown,
            Some(NamedKey::ArrowDown),
            Location::Standard,
        ),
        H::UP_ARROW => (Code::ArrowUp, Some(NamedKey::ArrowUp), Location::Standard),
        H::NUM_LOCK => (Code::NumLock, Some(NamedKey::NumLock), Location::Numpad),
        H::NUMPAD_DIVIDE => (Code::NumpadDivide, None, Location::Numpad),
        H::NUMPAD_MULTIPLY => (Code::NumpadMultiply, None, Location::Numpad),
        H::NUMPAD_SUBTRACT => (Code::NumpadSubtract, None, Location::Numpad),
        H::NUMPAD_ADD => (Code::NumpadAdd, None, Location::Numpad),
        H::NUMPAD_ENTER => (Code::NumpadEnter, Some(NamedKey::Enter), Location::Numpad),
        H::NUMPAD_DECIMAL => (Code::NumpadDecimal, None, Location::Numpad),
        H::NUMPAD_EQUAL => (Code::NumpadEqual, None, Location::Numpad),
        H::CONTROL_LEFT => (Code::ControlLeft, Some(NamedKey::Control), Location::Left),
        H::SHIFT_LEFT => (Code::ShiftLeft, Some(NamedKey::Shift), Location::Left),
        H::ALT_LEFT => (Code::AltLeft, Some(NamedKey::Alt), Location::Left),
        H::META_LEFT => (Code::MetaLeft, Some(NamedKey::Meta), Location::Left),
        H::CONTROL_RIGHT => (Code::ControlRight, Some(NamedKey::Control), Location::Right),
        H::SHIFT_RIGHT => (Code::ShiftRight, Some(NamedKey::Shift), Location::Right),
        H::ALT_RIGHT => (Code::AltRight, Some(NamedKey::Alt), Location::Right),
        H::META_RIGHT => (Code::MetaRight, Some(NamedKey::Meta), Location::Right),
        _ => (
            Code::Unidentified,
            Some(NamedKey::Unidentified),
            Location::Standard,
        ),
    }
}

/// Convert two cumulative UIKit pinch scales into a `ui-events` pinch delta.
///
/// UIKit reports `UIPinchGestureRecognizer::scale` as a cumulative multiplier
/// since the recognizer began. `ui-events` stores pinch as a per-update scale
/// delta, so callers must provide the previous cumulative scale.
///
/// Returns `None` if either scale is non-finite or non-positive.
pub fn pinch_delta_from_cumulative_scale(previous_scale: f64, current_scale: f64) -> Option<f32> {
    if !previous_scale.is_finite()
        || !current_scale.is_finite()
        || previous_scale <= 0.0
        || current_scale <= 0.0
    {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "UIKit gesture values are f64; ui-events uses f32"
    )]
    Some((current_scale / previous_scale - 1.0) as f32)
}

/// Convert two cumulative UIKit rotation values into a `ui-events` rotation
/// delta.
///
/// UIKit reports `UIRotationGestureRecognizer::rotation` as a cumulative
/// counterclockwise angle in radians since the recognizer began. `ui-events`
/// stores rotation as a per-update clockwise delta, so callers must provide
/// the previous cumulative rotation.
///
/// Returns `None` if either angle is non-finite.
pub fn rotation_delta_from_cumulative_rotation(
    previous_rotation_ccw: f64,
    current_rotation_ccw: f64,
) -> Option<f32> {
    if !previous_rotation_ccw.is_finite() || !current_rotation_ccw.is_finite() {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "UIKit gesture values are f64; ui-events uses f32"
    )]
    Some((previous_rotation_ccw - current_rotation_ccw) as f32)
}

/// Derive a normalized touch/stylus pressure in the range 0..=1.
///
/// - If `active` is false, returns 0.0.
/// - If a calibrated force is available (`force_along_axis`, `max_possible_force`), uses
///   `force/max`, clamped to 0..=1.
/// - For stylus input, if `altitude_angle` is provided, converts the force along the stylus axis
///   into a force perpendicular to the surface by multiplying by `sin(altitude_angle)`.
/// - Otherwise, returns a conservative default of 0.5 for active touches.
pub fn pressure_from_force(
    active: bool,
    force_along_axis: Option<f64>,
    max_possible_force: Option<f64>,
    altitude_angle: Option<f64>,
    is_stylus: bool,
) -> f64 {
    if !active {
        return 0.0;
    }
    let Some(force) = force_along_axis else {
        return 0.5;
    };
    let Some(max) = max_possible_force else {
        return 0.5;
    };
    if !force.is_finite() || !max.is_finite() || max <= 0.0 {
        return 0.5;
    }
    let mut p = (force / max).clamp(0.0, 1.0);
    if is_stylus {
        if let Some(alt) = altitude_angle {
            let s = sine(alt);
            if s.is_finite() {
                p *= s.clamp(0.0, 1.0);
            }
        }
    }
    p.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_inactive_is_zero() {
        assert_eq!(
            pressure_from_force(false, Some(1.0), Some(1.0), None, false),
            0.0
        );
    }

    #[test]
    fn pressure_defaults_when_force_missing() {
        assert_eq!(pressure_from_force(true, None, None, None, false), 0.5);
        assert_eq!(pressure_from_force(true, Some(1.0), None, None, false), 0.5);
        assert_eq!(
            pressure_from_force(true, Some(1.0), Some(0.0), None, false),
            0.5
        );
        assert_eq!(
            pressure_from_force(true, Some(1.0), Some(f64::NAN), None, false),
            0.5
        );
    }

    #[test]
    fn pressure_normalizes_and_clamps() {
        assert_eq!(
            pressure_from_force(true, Some(2.0), Some(4.0), None, false),
            0.5
        );
        assert_eq!(
            pressure_from_force(true, Some(10.0), Some(4.0), None, false),
            1.0
        );
        assert_eq!(
            pressure_from_force(true, Some(-1.0), Some(4.0), None, false),
            0.0
        );
    }

    #[test]
    fn stylus_altitude_scales_force_to_perpendicular() {
        // altitude = pi/2 => sin=1 (no change)
        let p = pressure_from_force(
            true,
            Some(1.0),
            Some(2.0),
            Some(core::f64::consts::FRAC_PI_2),
            true,
        );
        assert!((p - 0.5).abs() < 1e-12);

        // altitude = 0 => sin=0 (parallel)
        let p = pressure_from_force(true, Some(1.0), Some(2.0), Some(0.0), true);
        assert!((p - 0.0).abs() < 1e-12);
    }

    #[test]
    fn contact_geometry_defaults_to_single_pixel() {
        assert_eq!(
            contact_geometry_from_points(None, None, 2.0),
            PhysicalSize {
                width: 2.0,
                height: 2.0,
            }
        );
    }

    #[test]
    fn contact_geometry_scales_explicit_points() {
        assert_eq!(
            contact_geometry_from_points(Some(3.0), Some(4.0), 1.5),
            PhysicalSize {
                width: 4.5,
                height: 6.0,
            }
        );
    }

    #[test]
    fn platform_pointer_ids_do_not_collide_with_primary() {
        let pointer = pointer_info_from_platform_pointer_id(PointerType::Touch, 0);
        assert_eq!(
            pointer
                .pointer_id
                .expect("platform ids should be assigned")
                .get_inner()
                .get(),
            2
        );
        assert!(!pointer.is_primary_pointer());
    }

    #[test]
    fn uikit_modifier_bits_map_to_ui_events_modifiers() {
        let mods = modifiers_from_uikit_modifier_bits(
            UIKIT_CONTROL_BIT | UIKIT_ALT_BIT | UIKIT_SHIFT_BIT | UIKIT_META_BIT,
        );
        assert!(mods.ctrl());
        assert!(mods.alt());
        assert!(mods.shift());
        assert!(mods.meta());
    }

    #[test]
    fn hid_usage_maps_modifier_locations() {
        assert_eq!(
            hid_usage_to_code_named_location(hid::SHIFT_LEFT),
            (Code::ShiftLeft, Some(NamedKey::Shift), Location::Left)
        );
        assert_eq!(
            hid_usage_to_code_named_location(hid::META_RIGHT),
            (Code::MetaRight, Some(NamedKey::Meta), Location::Right)
        );
    }

    #[test]
    fn hid_usage_maps_numpad_keys() {
        assert_eq!(
            hid_usage_to_code_named_location(hid::NUMPAD_1),
            (Code::Numpad1, None, Location::Numpad)
        );
        assert_eq!(
            hid_usage_to_code_named_location(hid::NUMPAD_ENTER),
            (Code::NumpadEnter, Some(NamedKey::Enter), Location::Numpad)
        );
        assert_eq!(
            hid_usage_to_code_named_location(hid::NUMPAD_EQUAL),
            (Code::NumpadEqual, None, Location::Numpad)
        );
    }

    #[test]
    fn hid_usage_maps_function_named_keys() {
        assert_eq!(
            hid_usage_to_code_named_location(hid::F1),
            (Code::F1, Some(NamedKey::F1), Location::Standard)
        );
        assert_eq!(
            hid_usage_to_code_named_location(hid::F13),
            (Code::F13, Some(NamedKey::F13), Location::Standard)
        );
        assert_eq!(
            hid_usage_to_code_named_location(hid::F24),
            (Code::F24, Some(NamedKey::F24), Location::Standard)
        );
    }

    #[test]
    fn pinch_delta_from_cumulative_scale_uses_scale_ratio() {
        assert_eq!(pinch_delta_from_cumulative_scale(1.0, 1.25), Some(0.25));
        assert_eq!(pinch_delta_from_cumulative_scale(2.0, 3.0), Some(0.5));
        assert_eq!(pinch_delta_from_cumulative_scale(2.0, 1.0), Some(-0.5));
    }

    #[test]
    fn pinch_delta_from_cumulative_scale_rejects_invalid_values() {
        assert_eq!(pinch_delta_from_cumulative_scale(0.0, 1.0), None);
        assert_eq!(pinch_delta_from_cumulative_scale(1.0, 0.0), None);
        assert_eq!(pinch_delta_from_cumulative_scale(f64::NAN, 1.0), None);
        assert_eq!(pinch_delta_from_cumulative_scale(1.0, f64::INFINITY), None);
    }

    #[test]
    fn rotation_delta_from_cumulative_rotation_flips_to_clockwise_delta() {
        assert_eq!(
            rotation_delta_from_cumulative_rotation(0.25, 0.75),
            Some(-0.5)
        );
        assert_eq!(
            rotation_delta_from_cumulative_rotation(0.75, 0.25),
            Some(0.5)
        );
    }

    #[test]
    fn rotation_delta_from_cumulative_rotation_rejects_invalid_values() {
        assert_eq!(rotation_delta_from_cumulative_rotation(f64::NAN, 0.0), None);
        assert_eq!(
            rotation_delta_from_cumulative_rotation(0.0, f64::INFINITY),
            None
        );
    }

    #[test]
    fn uikit_pen_contact_maps_to_primary_button() {
        assert_eq!(
            contact_button_for_uikit_pointer(PointerType::Pen),
            Some(PointerButton::Primary)
        );

        let buttons = buttons_from_bitmask(contact_buttons_mask_for_uikit_pointer(
            PointerType::Pen,
            true,
            PointerButton::Secondary as u64,
        ));
        assert!(buttons.contains(PointerButton::Primary));
        assert!(buttons.contains(PointerButton::Secondary));
    }

    #[test]
    fn uikit_touch_contact_stays_buttonless() {
        assert_eq!(contact_button_for_uikit_pointer(PointerType::Touch), None);
        assert_eq!(
            contact_buttons_mask_for_uikit_pointer(PointerType::Touch, true, 0),
            0
        );
    }
}
