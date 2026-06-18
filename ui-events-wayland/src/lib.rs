// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bridges Wayland input into the [`ui-events`] model.
//!
//! This crate is Linux-only in practice. The `wayland-client` protocol bindings
//! it builds on are depended upon only on `cfg(target_os = "linux")` targets,
//! and the stateful reducers that consume them are gated the same way. On every
//! other target the crate compiles to an essentially empty library, so it can
//! live in a cross-platform workspace without pulling in platform dependencies.
//!
//! ## Architecture
//!
//! - [`mapping`] holds platform-neutral, value-based conversions from Wayland
//!   input primitives (evdev button codes, surface-local coordinates, axis
//!   values, modifier booleans) into [`ui-events`] building blocks. It is
//!   always compiled, performs no foreign-function calls, and references no
//!   `wayland-client` types, so its unit tests run on every target.
//! - Stateful reducers that consume `wayland-client` event streams are gated to
//!   `cfg(target_os = "linux")` and build on these helpers.
//!
//! ## Coordinates, scale, and time
//!
//! Wayland delivers surface-local pointer coordinates as logical (post-scale)
//! `f64` values. Callers supply a `scale_factor` (for example from
//! `wp_fractional_scale_v1`'s preferred scale divided by 120, or an output's
//! integer scale), and these helpers multiply by it to produce the physical
//! pixels [`ui-events`] expects.
//!
//! Reducers built on these helpers take a caller-provided monotonic nanosecond
//! timestamp for the pointer [`PointerState`], rather than Wayland's 32-bit
//! millisecond event timestamps, so input shares one clock domain with frame
//! sampling and timers.
//!
//! [`PointerState`]: ui_events::pointer::PointerState
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

pub mod mapping;

#[cfg(target_os = "linux")]
pub mod gesture;

#[cfg(target_os = "linux")]
pub mod pointer;

#[cfg(target_os = "linux")]
pub mod touch;
