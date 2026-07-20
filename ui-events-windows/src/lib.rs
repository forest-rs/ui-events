// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This crate bridges the raw [Win32 API] window messages (mouse, touch, keyboard, IME, etc.)
//! into the [`ui-events`] model.
//!
//! The primary entry point is `WindowMessageReducer`.
//!
//! Call `WindowMessageReducer::reduce` with nanoseconds in the host clock domain so input, timers,
//! frame sampling, submission timestamps, and diagnostics can share one timeline.
//! The timestamp must be real monotonic nanoseconds, not milliseconds, microseconds, frame counts,
//! or a constant value. Tap counting uses it for nanosecond-duration thresholds.
//!
//! Each call returns both zero or more translations and a `MessageResponse` that tells the
//! window procedure whether to forward or consume the native message. The distinction matters
//! for messages such as `WM_TOUCH`, whose native handle is consumed by the reducer, and
//! `WM_XBUTTON*`, which requires a nonzero result when handled.
//!
//! A single raw Win32 message can produce more than one normalized event (for example,
//! the first `WM_MOUSEMOVE` after the cursor entered the window produces a synthetic
//! `PointerEvent::Enter` followed by the `Move`, and a single `WM_TOUCH` message can carry
//! more than one simultaneous touch point).
//!
//! Text input uses the immediate Win32 message model: `WM_CHAR` supplies committed text and
//! IMM32 `WM_IME_*` messages supply composition updates. This crate does not implement a
//! host-integrated TSF text store or own editor state.
//!
//! A text-editing host enables IME input while an editor has focus and keeps the native candidate
//! UI aligned with its insertion point:
//!
//! ```no_run
//! # #[cfg(windows)]
//! # mod windows_example {
//! use dpi::{LogicalPosition, LogicalSize};
//! use ui_events_windows::{ImeAssociationError, InputEvent, WindowMessageReducer};
//! use windows_sys::Win32::Foundation::HWND;
//!
//! fn configure_text_input(
//!     hwnd: HWND,
//!     dispatch: impl FnOnce(InputEvent),
//! ) -> Result<WindowMessageReducer, ImeAssociationError> {
//!     // SAFETY: The reducer is created for this window and remains on its window thread.
//!     let mut reducer = unsafe { WindowMessageReducer::new(hwnd, 2.0) };
//!     if let Some(event) = reducer.set_ime_allowed(true)? {
//!         dispatch(event);
//!     }
//!     reducer.set_ime_cursor_area(
//!         LogicalPosition::new(40.0, 24.0),
//!         LogicalSize::new(1.0, 18.0),
//!     );
//!     Ok(reducer)
//! }
//! # }
//! ```
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
mod keyboard;
#[cfg(windows)]
mod pointer;
#[cfg(windows)]
mod text;

#[cfg(windows)]
mod reducer;

#[cfg(windows)]
pub use reducer::{
    ImeAssociationError, InputEvent, MessageResponse, Reduction, WindowMessageReducer,
};
