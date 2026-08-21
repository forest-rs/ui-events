// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bridges X11 input into the [`ui-events`] model.
//!
//! Input goes through XInput 2 and XKB.
//! There is no core-protocol fallback.
//! The crate owns no connection: select XInput 2 events, pump the queue, and
//! pass each decoded [`x11rb` event] to the [`keyboard`], [`pointer`],
//! [`touch`], [`gesture`], and [`xim`] reducers.
//! Pointer, touch, and gesture take a scale factor and a monotonic nanosecond
//! timestamp in the host clock.
//! [`xim`] is an XIM client on the same connection.
//! [`mapping`] holds the plain-value conversions.
//!
//! Physical key codes need no system library.
//! The `xkb` feature links `libxkbcommon` (via [`ui-events-xkb`]) for logical
//! keys, text, and full modifier state from a keymap.
//!
//! `x11rb` defaults to pure-Rust [`RustConnection`].
//! The `xcb` feature switches to [`XCBConnection`] for sharing an existing XCB
//! connection; `dl-libxcb` loads `libxcb` at run time.
//!
//! [`RustConnection`]: https://docs.rs/x11rb/latest/x11rb/rust_connection/struct.RustConnection.html
//! [`XCBConnection`]: https://docs.rs/x11rb/latest/x11rb/xcb_ffi/struct.XCBConnection.html
//! [`x11rb` event]: https://docs.rs/x11rb/latest/x11rb/protocol/enum.Event.html
//! [`pointer`]: crate::pointer
//! [`gesture`]: crate::gesture
//! [`xim`]: crate::xim
//! [`ui-events-xkb`]: https://docs.rs/ui-events-xkb/
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
#![no_std]

extern crate alloc;

pub mod mapping;

#[cfg(unix)]
pub mod keyboard;

#[cfg(unix)]
pub mod pointer;

#[cfg(unix)]
pub mod touch;

#[cfg(unix)]
pub mod gesture;

#[cfg(unix)]
pub mod xim;

#[cfg(unix)]
mod tap;
