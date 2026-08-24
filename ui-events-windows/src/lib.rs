// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This crate bridges the raw [Win32 API] window messages (mouse, touch, keyboard, IME, etc.)
//! into the [`ui-events`] model.
//!
//! The primary entry point is [`EventReducer`].
//!
//! Call [`EventReducer::reduce`] with nanoseconds in the host clock domain so input, timers,
//! frame sampling, submission timestamps, and diagnostics can share one timeline.
//! The timestamp must be real monotonic nanoseconds, not milliseconds, microseconds, frame counts,
//! or a constant value. Tap counting uses it for nanosecond-duration thresholds.
//!
//! [`EventReducer::reduce`] returns a `Vec` of zero or more translations.
//! A single raw Win32 message can produce more than one normalized event (for example,
//! the first `WM_MOUSEMOVE` after the cursor entered the window produces a synthetic
//! `PointerEvent::Enter` followed by the `Move`, and a single `WM_TOUCH` message can carry
//! more than one simultaneous touch point).
//!
//! This crate also handles some side-effecting Win32 calls:
//!   - It calls `TrackMouseEvent` on mouse enter so that `WM_MOUSELEAVE` is delivered.
//!   - It calls `SetCapture`/`ReleaseCapture` around button presses so that a drag that leaves
//!     the window still delivers its button-up.
//!
//! [`ui-events`]: https://docs.rs/ui-events/

// LINEBENDER LINT SET - lib.rs - v3
// See https://linebender.org/wiki/canonical-lints/
// These lints shouldn't apply to examples or tests.
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
// These lints shouldn't apply to examples.
#![warn(clippy::print_stdout, clippy::print_stderr)]
// Targeting e.g. 32-bit means structs containing usize can give false positives for 64-bit.
#![cfg_attr(target_pointer_width = "64", warn(clippy::trivially_copy_pass_by_ref))]
// END LINEBENDER LINT SET
#![cfg_attr(
    windows,
    expect(unsafe_code, reason = "Bridging the raw Win32 API requires FFI calls.")
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
pub use reducer::{Event, EventReducer};

#[cfg(not(windows))]
pub use dummy::EventReducer;

#[cfg(not(windows))]
mod dummy {
    /// Dummy type.
    #[derive(Debug)]
    pub struct EventReducer {}
    impl EventReducer {
        /// Dummy function.
        pub fn reduce() {}
    }
}
