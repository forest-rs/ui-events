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
//! Hosts opt into raw touch by calling `RegisterTouchWindow` for their window
//! and passing every recognized message to the reducer. Promoted touch mouse
//! messages are suppressed only for registered windows; other windows retain
//! Windows' mouse fallback. Registration and window lifetime belong to the host.
//!
//! Text input uses the immediate Win32 message model: `WM_CHAR` supplies committed text and
//! IMM32 `WM_IME_*` messages supply composition updates. This crate does not implement a
//! host-integrated TSF text store or own editor state.
//!
//! A text-editing host enables IME input while an editor has focus and keeps the native candidate
//! UI aligned with its insertion point:
//!
//! ```no_run
//! # #[cfg(all(windows, feature = "std"))]
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

//!
//! Portable TSF value adapters are available in the `tsf` module on every target,
//! including `no_std` builds with default features disabled. The Win32
//! message reducer requires Windows and the `std` feature (enabled by default).
//! The helpers adapt coherent editor snapshots, UTF-16 text ranges, selection
//! direction, replacements, and screen geometry for a host-owned text store.
//! A host implementing TSF must also own the native COM text-store lifecycle;
//! see `NATIVE_INTEGRATION.md`.

// LINEBENDER LINT SET - lib.rs - v3
// See https://linebender.org/wiki/canonical-lints/
// These lints shouldn't apply to examples or tests.
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
// These lints shouldn't apply to examples.
#![warn(clippy::print_stdout, clippy::print_stderr)]
// Targeting e.g. 32-bit means structs containing usize can give false positives for 64-bit.
#![cfg_attr(target_pointer_width = "64", warn(clippy::trivially_copy_pass_by_ref))]
// END LINEBENDER LINT SET
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(
    all(windows, feature = "std"),
    expect(unsafe_code, reason = "Bridging the raw Win32 API requires FFI calls.")
)]

#[cfg(all(windows, feature = "std"))]
mod keyboard;
#[cfg(all(windows, feature = "std"))]
mod pointer;
#[cfg(all(windows, feature = "std"))]
mod text;

#[cfg(all(windows, feature = "std"))]
mod reducer;

#[cfg(all(windows, feature = "std"))]
pub use reducer::{
    ImeAssociationError, InputEvent, MessageResponse, Reduction, WindowMessageReducer,
};

extern crate alloc;

pub mod tsf;

pub use tsf::{
    TSF_DOCUMENT_END, TsfActiveSelectionEnd, TsfDocumentSnapshot, TsfRange, TsfScreenRect,
    TsfSelection, TsfTextChange, TsfTextExtent, TsfTextReplacement, TsfTextResult,
    exact_acp_from_screen_point, insert_text_at_selection, nearest_acp_from_screen_point,
    replace_text, replace_utf16, selection_from_host,
};
