// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This crate bridges the raw [Win32 API] window messages (mouse, touch,
//! keyboard, IME, etc.) into the [`ui-events`] model.
//!
//! It is a Windows-native sibling of [`ui-events-winit`]: instead of
//! converting `winit`'s `WindowEvent`s, it converts the `WM_*` messages a
//! `WNDPROC` receives directly, inlining the same Win32-API-to-normalized-event
//! conversions that the [`winit-win32`] backend performs internally to build
//! its `winit` events in the first place.
//!
//! The primary entry point is [`WindowEventReducer`].
//!
//! Call [`WindowEventReducer::reduce`] with nanoseconds in the host clock
//! domain so input, timers, frame sampling, submission timestamps, and
//! diagnostics can share one timeline.
//! The timestamp must be real monotonic nanoseconds, not milliseconds,
//! microseconds, frame counts, or a constant value; tap counting uses
//! nanosecond-duration thresholds.
//!
//! Unlike [`ui-events-winit`]'s reducer, [`WindowEventReducer::reduce`] here
//! returns a `Vec` of zero or more translations rather than a single
//! `Option`: a single raw Win32 message can legitimately produce more than
//! one normalized event (for example, the first `WM_MOUSEMOVE` after the
//! cursor entered the window produces a synthetic `PointerEvent::Enter`
//! followed by the `Move`, and a single `WM_TOUCH` message can carry more
//! than one simultaneous touch point).
//!
//! This crate also takes on a few side-effecting Win32 calls that `winit`'s
//! windowing layer would otherwise be responsible for, since there is no
//! such layer here: it calls `TrackMouseEvent` so that `WM_MOUSELEAVE` is
//! delivered, and it calls `SetCapture`/`ReleaseCapture` around button
//! presses so that a drag that leaves the window still delivers its button-up.
//!
//! All of this crate's functionality requires the `windows` target family;
//! on any other target, this crate still compiles, but is empty, the same
//! way `ui-events-appkit`'s `objc2`-dependent items are gated to
//! `target_os = "macos"`.
//!
//! [`ui-events`]: https://docs.rs/ui-events/
//! [`ui-events-winit`]: https://docs.rs/ui-events-winit/
//! [Win32 API]: https://learn.microsoft.com/en-us/windows/win32/api/
//! [`winit-win32`]: https://github.com/rust-windowing/winit

// LINEBENDER LINT SET - lib.rs - v3
// See https://linebender.org/wiki/canonical-lints/
// These lints shouldn't apply to examples or tests.
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
// These lints shouldn't apply to examples.
#![warn(clippy::print_stdout, clippy::print_stderr)]
// Targeting e.g. 32-bit means structs containing usize can give false positives for 64-bit.
#![cfg_attr(target_pointer_width = "64", warn(clippy::trivially_copy_pass_by_ref))]
// END LINEBENDER LINT SET
#![allow(
    unsafe_code,
    reason = "Bridging the raw Win32 API requires FFI calls."
)]

#[cfg(windows)]
pub mod keyboard;
#[cfg(windows)]
pub mod pointer;
#[cfg(windows)]
pub mod text;

#[cfg(windows)]
mod reducer;

#[cfg(windows)]
pub use reducer::{WindowEventReducer, WindowEventTranslation};
