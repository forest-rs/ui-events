// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! AppKit-specific mapping helpers for building `ui-events` from raw values.

use ui_events::pointer::PointerOrientation;
pub use ui_events_apple_common::{
    buttons_from_bitmask, modifiers_from_bools, pointer_info_from_platform_ids,
    pointer_info_primary_for_type, pointer_scroll_delta_from_raw, pointer_state_from_raw,
    try_from_button_index,
};

#[cfg(feature = "std")]
fn hypot(x: f64, y: f64) -> f64 {
    x.hypot(y)
}

#[cfg(all(not(feature = "std"), feature = "libm"))]
fn hypot(x: f64, y: f64) -> f64 {
    libm::hypot(x, y)
}

#[cfg(feature = "std")]
fn sqrt(value: f64) -> f64 {
    value.sqrt()
}

#[cfg(all(not(feature = "std"), feature = "libm"))]
fn sqrt(value: f64) -> f64 {
    libm::sqrt(value)
}

#[cfg(feature = "std")]
fn atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}

#[cfg(all(not(feature = "std"), feature = "libm"))]
fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

#[cfg(all(not(feature = "std"), not(feature = "libm")))]
compile_error!("ui-events-appkit requires either the `std` or `libm` feature");

fn wrap_angle_positive(angle: f64) -> f64 {
    let tau = core::f64::consts::TAU;
    let wrapped = angle % tau;
    if wrapped < 0.0 {
        wrapped + tau
    } else {
        wrapped
    }
}

/// Convert AppKit tablet tilt fractions into a [`PointerOrientation`].
///
/// AppKit tablet events report `NSEvent::tilt` as an x/y fraction in
/// `-1.0..=1.0`, where `0.0` is perpendicular and `1.0` is parallel to the
/// surface along that axis. This converts that normalized vector into
/// spherical altitude/azimuth:
/// - altitude: 0 is parallel to the surface, π/2 is perpendicular.
/// - azimuth: 0 is +x, π/2 is +y.
pub fn orientation_from_tilt_fraction(tilt_x: f64, tilt_y: f64) -> PointerOrientation {
    let x = finite_or(tilt_x, 0.0).clamp(-1.0, 1.0);
    let y = finite_or(tilt_y, 0.0).clamp(-1.0, 1.0);
    let r = hypot(x, y).clamp(0.0, 1.0);
    let z = sqrt((1.0 - r * r).max(0.0));
    let altitude = if r == 0.0 {
        core::f64::consts::FRAC_PI_2
    } else {
        atan2(z, r)
    };
    let azimuth = if r == 0.0 {
        core::f64::consts::FRAC_PI_2
    } else {
        wrap_angle_positive(atan2(y, x))
    };

    #[expect(
        clippy::cast_possible_truncation,
        reason = "AppKit-derived angles are finite; ui-events stores orientation as f32"
    )]
    let altitude = altitude as f32;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "AppKit-derived angles are finite; ui-events stores orientation as f32"
    )]
    let azimuth = azimuth as f32;

    PointerOrientation { altitude, azimuth }
}

fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_events::pointer::{PointerId, PointerType};

    #[test]
    fn tilt_fraction_zero_is_perpendicular_default_azimuth() {
        let o = orientation_from_tilt_fraction(0.0, 0.0);
        assert!((o.altitude as f64 - core::f64::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((o.azimuth as f64 - core::f64::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn full_x_tilt_fraction_is_parallel() {
        let o = orientation_from_tilt_fraction(1.0, 0.0);
        assert!((o.azimuth as f64 - 0.0).abs() < 1e-6);
        assert!((o.altitude as f64 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn full_y_tilt_fraction_is_parallel() {
        let o = orientation_from_tilt_fraction(0.0, 1.0);
        assert!((o.azimuth as f64 - core::f64::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((o.altitude as f64 - 0.0).abs() < 1e-6);
    }

    #[test]
    fn half_x_tilt_fraction_has_sixty_degree_altitude() {
        let o = orientation_from_tilt_fraction(0.5, 0.0);
        assert!((o.azimuth as f64 - 0.0).abs() < 1e-6);
        assert!((o.altitude as f64 - core::f64::consts::FRAC_PI_3).abs() < 1e-6);
    }

    #[test]
    fn non_finite_tilt_fraction_defaults_to_perpendicular() {
        let o = orientation_from_tilt_fraction(f64::NAN, f64::INFINITY);
        assert!((o.altitude as f64 - core::f64::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((o.azimuth as f64 - core::f64::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn platform_pointer_ids_do_not_collide_with_primary() {
        let pointer = pointer_info_from_platform_ids(PointerType::Pen, Some(0), Some(0));
        assert_ne!(pointer.pointer_id, Some(PointerId::PRIMARY));
        assert!(!pointer.is_primary_pointer());
    }
}
