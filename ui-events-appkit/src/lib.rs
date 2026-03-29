// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! AppKit (macOS) adapter using `objc2`.
//!
//! This crate provides lightweight helpers to convert AppKit events into
//! [`ui-events`] types, mirroring the style of the `ui-events-web` and
//! `ui-events-winit` adapters.
//!
//! It does not provide an `NSView` implementation for you. Instead, your
//! AppKit view or responder should call these helpers from the corresponding
//! AppKit callbacks.
//!
//! Currently supported:
//!
//! - Pointer (mouse/pen) down/up/move/enter/leave/scroll
//! - Trackpad gestures: magnify (pinch) and rotate
//! - Keyboard down/up
//! - `AppKitInputResponder`, a reusable `NSResponder` for pointer, scroll,
//!   gesture, tablet, and keyboard input
//!
//! `AppKitInputResponder` intentionally does not implement text input, IME, or
//! edit-command protocols.
//!
//! ## Feature Policy
//!
//! - `std` is enabled by default.
//! - `libm` is required for `no_std` builds because tablet tilt orientation uses
//!   trigonometry.
//! - With `default-features = false`, the crate remains `no_std` with `alloc`
//!   and still compiles on non-macOS targets because the `objc2` dependencies
//!   are behind `target_os = "macos"`.
//! - On macOS targets, the `objc2*` crates are compiled with their `std` feature
//!   enabled to support Objective-C runtime integration.
//! - Defaults avoid pulling in unnecessary APIs by disabling default features
//!   for `objc2*` crates and enabling only the AppKit symbols this crate uses.
//!
//! ## Pointer Notes
//!
//! - Pointer positions come from `NSEvent::locationInWindow`, scaled from
//!   points into physical pixels but not converted into a specific `NSView`.
//!   `pointer_event_from_nsevent_at_position` accepts a caller-provided
//!   coordinate, and `AppKitInputResponderHost` requires the host to provide
//!   the position in its own coordinate space.
//! - Event timestamps use AppKit's monotonic seconds-since-boot timebase, not
//!   Unix epoch time.
//! - Mouse button down/up click counts use AppKit's `clickCount` directly.
//!   Other pointer event kinds, including scroll events, use `0` without
//!   querying click metadata from AppKit.
//! - Mouse events may query AppKit tablet metadata when AppKit reports tablet
//!   mouse-event subtypes. Scroll events do not query tablet metadata.
//! - Tablet tilt uses AppKit's `NSEvent::tilt` fractions, where `0.0` is
//!   perpendicular and `1.0` is parallel to the surface along that axis.
//!
//! ## Scroll Deltas
//!
//! - Scroll deltas preserve AppKit's `scrollingDeltaX/Y` sign, which already
//!   reflects the user's natural-scrolling preference and matches the macOS
//!   path used by `winit`.
//! - When `hasPreciseScrollingDeltas` is true, deltas map to
//!   [`ScrollDelta::PixelDelta`]. Otherwise, they map to
//!   [`ScrollDelta::LineDelta`].
//!
//! ## Gestures
//!
//! - Magnify gesture events map to [`PointerGesture::Pinch`](ui_events::pointer::PointerGesture::Pinch)
//!   using AppKit's per-event `magnification` delta.
//! - Rotate gesture events map to
//!   [`PointerGesture::Rotate`](ui_events::pointer::PointerGesture::Rotate) in
//!   clockwise radians.
//!
//! ## Tablet Events
//!
//! - `NSEventType::TabletPoint` maps to [`PointerType::Pen`] with pressure,
//!   tangential pressure, and orientation derived from AppKit tilt fractions.
//! - `NSEventType::TabletProximity` maps to `PointerEvent::Enter` or
//!   `PointerEvent::Leave` based on `isEnteringProximity`.
//!
//! ## Keyboard Notes
//!
//! - `flagsChanged` uses aggregate modifier flags. Releasing one side of a
//!   modifier while the other side remains held may still report the key as
//!   down until this adapter grows device-dependent left/right modifier masks.
//!
//! ## High-Level Helpers
//!
//! - `AppKitInputResponder`
//! - `pointer_event_from_nsevent`
//! - `pointer_event_from_nsevent_at_position`
//! - `keyboard_event_from_nsevent`
//!
//! If you prefer, low-level mappers in [`mapping`] let you build events from
//! raw values (e.g. coordinates, button number, modifier booleans) without
//! pulling in AppKit types in your own code.
//!
//! [`PointerType::Pen`]: ui_events::pointer::PointerType::Pen
//! [`ScrollDelta::LineDelta`]: ui_events::ScrollDelta::LineDelta
//! [`ScrollDelta::PixelDelta`]: ui_events::ScrollDelta::PixelDelta
//! [`ui-events`]: https://docs.rs/ui-events/

#![allow(unsafe_code, reason = "We access platform libraries using ffi.")]
#![no_std]

extern crate alloc;

pub mod mapping;

#[cfg(target_os = "macos")]
pub mod appkit;
#[cfg(target_os = "macos")]
pub mod input_responder;
#[cfg(target_os = "macos")]
pub use appkit::{
    keyboard_event_from_nsevent, pointer_event_from_nsevent, pointer_event_from_nsevent_at_position,
};
#[cfg(target_os = "macos")]
pub use input_responder::{AppKitInputResponder, AppKitInputResponderHost};
