// Copyright 2025 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Cross-platform input event types modeled after W3C UI Events.
//!
//! This crate provides small, portable data types for working with pointer
//! (mouse, touch, pen) and keyboard input in a platform-agnostic way.
//! It aims to closely follow W3C terminology while remaining practical for native
//! application development.
//!
//! ## What you get:
//!
//! - Pointer events: button down/up, move, enter/leave, scroll, gestures
//! - Rich pointer state: position, pressure, tilt, contact size, modifiers
//! - Keyboard types re-exported from [`keyboard-types`]
//! - Text-input events for soft keyboards and IMEs
//! - Edit-command events for semantic editor operations
//! - A stable vocabulary you can adapt from windowing backends
//!
//! This crate is intentionally focused on data structures — it does not open
//! windows or read events.
//! For integrations, see the adapter crates:
//!
//! - [`ui-events-winit`]: Convert between `winit` and `ui-events`.
//! - [`ui-events-web`]: Convert between Web (`web-sys`) DOM events and `ui-events`.
//!
//! ## Coordinate system and units
//!
//! - Positions are in physical pixels (`dpi::PhysicalPosition<f64>`), with the
//!   Y axis increasing downward.
//! - Use [`PointerState::logical_position`](pointer::PointerState::logical_position)
//!   to obtain logical coordinates using a scale factor.
//! - Scroll deltas are expressed via [`ScrollDelta`]; see its docs for details
//!   on page/line/pixel semantics.
//!
//! ## Primary pointer
//!
//! Some interactions need a notion of a “primary” pointer (e.g. left mouse button, first touch).
//! The reserved id [`PointerId::PRIMARY`](pointer::PointerId::PRIMARY) marks this.
//! Helper methods like [`PointerEvent::is_primary_pointer`](pointer::PointerEvent::is_primary_pointer)
//! and [`PointerInfo::is_primary_pointer`](pointer::PointerInfo::is_primary_pointer) are provided for convenience.
//!
//! ## Feature flags
//!
//! - `std` (default): Use the Rust standard library.
//! - `kurbo`: Add convenience methods for converting positions to `kurbo::Point`.
//!
//! ## Examples
//!
//! Basic matching on pointer events:
//!
//! ```
//! use ui_events::pointer::{PointerEvent, PointerButton, PointerButtonEvent, PointerInfo, PointerState, PointerType};
//! use ui_events::ScrollDelta;
//! use keyboard_types::Modifiers;
//! use dpi::{PhysicalPosition, PhysicalSize};
//!
//! fn handle_event(ev: PointerEvent) {
//!     match ev {
//!         PointerEvent::Down(PointerButtonEvent { button, state, .. }) => {
//!             if button == Some(PointerButton::Primary) {
//!                 // Start a drag, for example
//!                 let pos = state.position;
//!                 let _ = (pos.x, pos.y);
//!             }
//!         }
//!         PointerEvent::Move(upd) => {
//!             let logical = upd.current.logical_position();
//!             let _ = (logical.x, logical.y);
//!         }
//!         PointerEvent::Scroll(s) => {
//!             match s.delta {
//!                 ScrollDelta::PageDelta(x, y) => { let _ = (x, y); }
//!                 ScrollDelta::LineDelta(x, y) => { let _ = (x, y); }
//!                 ScrollDelta::PixelDelta(p) => { let _ = (p.x, p.y); }
//!             }
//!         }
//!         _ => {}
//!     }
//! }
//!
//! // Construct a minimal primary-pointer Down event
//! let ev = PointerEvent::Down(PointerButtonEvent{
//!     button: Some(PointerButton::Primary),
//!     pointer: PointerInfo{
//!         pointer_id: Some(ui_events::pointer::PointerId::PRIMARY),
//!         persistent_device_id: None,
//!         pointer_type: PointerType::Mouse,
//!     },
//!     state: PointerState{
//!         time: 0,
//!         position: PhysicalPosition { x: 10.0, y: 20.0 },
//!         buttons: Default::default(),
//!         modifiers: Modifiers::empty(),
//!         count: 1,
//!         contact_geometry: PhysicalSize { width: 1.0, height: 1.0 },
//!         orientation: Default::default(),
//!         pressure: 0.5,
//!         tangential_pressure: 0.0,
//!         scale_factor: 2.0,
//!     },
//! });
//! handle_event(ev);
//! ```
//!
//! ## See also
//!
//! - [`ui-events-winit`]
//! - [`ui-events-web`]
//! - [`keyboard-types`]
//!
//! [`keyboard-types`]: https://docs.rs/keyboard-types/
//! [`ui-events-winit`]: https://docs.rs/ui-events-winit/
//! [`ui-events-web`]: https://docs.rs/ui-events-web/
//! [`winit`]: https://docs.rs/winit/
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

pub mod edit;
pub mod keyboard;
pub mod pointer;
pub mod text;

mod scroll;

pub use scroll::ScrollDelta;
