// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UIKit (iOS/tvOS) adapter using `objc2`.
//!
//! This crate provides lightweight helpers to convert UIKit events into
//! [`ui-events`] types, mirroring the style of the `ui-events-web` and
//! `ui-events-winit` adapters.
//!
//! It does not provide a `UIView` implementation for you. Instead, your
//! `UIView` or responder should call these helpers from the corresponding
//! UIKit callbacks, or install `UIKitInputResponder` as a reusable responder
//! for touch, remote, and keyboard input.
//!
//! Currently supported:
//!
//! - Pointer (touch/stylus) down/up/move/cancel
//! - Pencil hover enter/move/leave when UIKit reports region phases
//! - tvOS remote presses → keyboard
//! - Hardware keyboard via `UIPress` + `UIKey`
//! - `UIKitInputResponder`, a reusable `UIResponder` for touch, remote, and
//!   keyboard input
//!
//! ## Feature Policy
//!
//! - `std` is enabled by default and uses the platform math intrinsics.
//! - `libm` is required for `no_std` builds because stylus pressure normalization uses `sin`.
//! - The crate is `no_std` with `alloc` when default features are disabled.
//!   The `objc2*` crates are still compiled with their `std` feature enabled on
//!   Apple mobile targets to support Objective-C runtime integration.
//! - Defaults avoid pulling in unnecessary APIs by disabling default features
//!   for `objc2*` crates and enabling only the UIKit symbols this crate uses.
//!
//! ## Pointer Notes
//!
//! - Touch positions come from `UITouch::preciseLocationInView(None)`, scaled
//!   from points into physical pixels. The caller chooses the view coordinate
//!   space by passing the view to UIKit before using lower-level mappers.
//! - Touch timestamps use UIKit's monotonic seconds-since-boot timebase, not
//!   Unix epoch time.
//! - Touch pressure is derived from `UITouch.force/maximumPossibleForce` when available.
//! - Stylus (Apple Pencil) pressure is converted from force along the stylus axis to
//!   perpendicular-to-surface pressure using `sin(altitudeAngle)`.
//! - Stylus orientation is mapped from `altitudeAngle`/`azimuthAngleInView`.
//! - Pencil hover maps to enter/move/leave when UIKit reports `RegionEntered`,
//!   `RegionMoved`, and `RegionExited`.
//! - Finger touches use `button: None`, matching the DOM `TouchEvent` path in
//!   `ui-events-web`. Active stylus contacts use [`PointerButton::Primary`](ui_events::pointer::PointerButton::Primary).
//! - UIKit exposes stylus `rollAngle`, but `ui-events` currently has no roll field.
//! - UIKit exposes estimated-property update APIs, but `ui-events` currently has no sample-revision
//!   metadata for correcting previously emitted stylus samples.
//! - UIKit does not expose a tangential-pressure value on `UITouch`, so
//!   [`PointerState::tangential_pressure`](ui_events::pointer::PointerState::tangential_pressure)
//!   remains `0.0` for this adapter.
//!
//! ## Gestures
//!
//! - Pinch gesture recognizers map to [`PointerGesture::Pinch`](ui_events::pointer::PointerGesture::Pinch)
//!   by differencing UIKit's cumulative `scale` against a caller-provided
//!   previous scale. Rotation gesture recognizers follow the same pattern with
//!   cumulative counterclockwise `rotation` values.
//!
//! ## High-Level Helpers
//!
//! - `UIKitInputResponder`
//! - `keyboard_event_from_uipress`
//! - `keyboard_event_from_uikey`
//! - `pointer_event_from_touch_and_event`
//! - `pointer_event_from_touch` (uncommon convenience helper)
//! - `pointer_gesture_from_uipinch` (feature: `gestures`)
//! - `pointer_gesture_from_uirotation` (feature: `gestures`)
//! - `mapping::pinch_delta_from_cumulative_scale` (feature: `gestures`)
//! - `mapping::rotation_delta_from_cumulative_rotation` (feature: `gestures`)
//!
//! If you prefer, low-level mappers in [`mapping`] let you build events from
//! raw values (e.g. coordinates, button number, modifier booleans) without
//! pulling in UIKit types in your own code.
//!
//! [`ui-events`]: https://docs.rs/ui-events/

#![allow(unsafe_code, reason = "We access platform libraries using ffi.")]
#![no_std]

extern crate alloc;

pub mod mapping;

#[cfg(any(target_os = "ios", target_os = "tvos"))]
pub mod input_responder;
#[cfg(any(target_os = "ios", target_os = "tvos"))]
pub mod uikit;

// Top-level re-exports for convenience.
#[cfg(any(target_os = "ios", target_os = "tvos"))]
pub use input_responder::{UIKitInputResponder, UIKitInputResponderHost};
#[cfg(any(target_os = "ios", target_os = "tvos"))]
pub use uikit::{keyboard_event_from_uikey, keyboard_event_from_uipress};
#[cfg(any(target_os = "ios", target_os = "tvos"))]
pub use uikit::{pointer_event_from_touch, pointer_event_from_touch_and_event};
#[cfg(all(any(target_os = "ios", target_os = "tvos"), feature = "gestures"))]
pub use uikit::{pointer_gesture_from_uipinch, pointer_gesture_from_uirotation};
