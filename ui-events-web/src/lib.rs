// Copyright 2025 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This crate bridges [`web_sys`] DOM input events — Pointer Events (mouse, touch, pen),
//! Wheel, and Keyboard — into the [`ui-events`] model.
//!
//! It provides lightweight helpers to convert browser events into portable
//! `ui-events` types you can feed into your input handling. It supports
//! Pointer Events (mouse, touch, pen), keyboard, and text/composition input.
//!
//! ## Keyboard
//!
//! - [`keyboard::from_web_keyboard_event`]
//! - Optional helpers: [`keyboard::from_web_keydown_event`], [`keyboard::from_web_keyup_event`]
//!
//! ## Text / Composition
//!
//! - [`text::text_event_from_dom_event`]
//! - Typed helpers: [`text::from_web_input_event`], [`text::from_web_composition_event`]
//!
//! ## Pointer (Pointer Events)
//!
//! - One‑shot DOM conversion: [`pointer::pointer_event_from_dom_event`]
//! - Multi-touch aware DOM conversion (may return multiple events):
//!   [`pointer::pointer_events_from_dom_event`]
//! - Per‑event helpers (preferred):
//!   [`pointer::down_from_pointer_event`], [`pointer::up_from_pointer_event`],
//!   [`pointer::move_from_pointer_event`], [`pointer::enter_from_pointer_event`],
//!   [`pointer::leave_from_pointer_event`], [`pointer::cancel_from_pointer_event`]
//! - Mouse‑only helpers (legacy and less portable):
//!   [`pointer::down_from_mouse_event`], [`pointer::up_from_mouse_event`],
//!   [`pointer::move_from_mouse_event`], [`pointer::enter_from_mouse_event`],
//!   [`pointer::leave_from_mouse_event`], [`pointer::scroll_from_wheel_event`]
//! - Conversion options: [`pointer::Options`] (controls scale/coalesced/predicted)
//! - Pointer capture helpers: [`pointer::set_pointer_capture`],
//!   [`pointer::release_pointer_capture`], [`pointer::has_pointer_capture`]
//!
//! ## Notes
//!
//! - Positions use `clientX` / `clientY` scaled by `Options::scale_factor`. Pass the
//!   current device-pixel-ratio for physical pixels.
//! - **Element-local coordinates (recipe):** DOM pointer coordinates are reported in viewport
//!   space even when you register the listener on an element. For canvas/SVG apps you often
//!   want coordinates relative to a specific element’s top-left corner. A common recipe is:
//!
//!   ```no_run
//!   # use web_sys::{window, Element, PointerEvent};
//!   # let (el, e): (Element, PointerEvent) = unimplemented!();
//!   let rect = el.get_bounding_client_rect();
//!   let dpr = window().unwrap().device_pixel_ratio();
//!   let x = (e.client_x() as f64 - rect.left()) * dpr;
//!   let y = (e.client_y() as f64 - rect.top()) * dpr;
//!   ```
//!
//!   This does *not* undo arbitrary CSS transforms, and for SVG you may want a true transform
//!   inversion (e.g. `getScreenCTM()`).
//! - Coalesced and predicted move samples are opt‑in via `Options`.
//! - Touch events (`touchstart`/`touchmove`/`touchend`/`touchcancel`) may correspond to multiple
//!   changed touches; use `pointer_events_from_dom_event` to receive all of them.
//! - Stylus fields:
//!   - `pressure`, `tangential_pressure`, `contact_geometry`, and `orientation` are populated
//!     from Pointer Events data when available (preferring `altitudeAngle`/`azimuthAngle`,
//!     otherwise falling back to `tiltX`/`tiltY`).
//!   - Stylus rotation/twist (Pointer Events `twist`) is not currently exposed by `ui-events`,
//!     so it is not mapped.
//! - Keyboard: unknown `key`/`code` map to `Unidentified`; `is_composing` reflects the DOM flag.
//! - Text:
//!   - `CompositionEvent` maps to composition update/end.
//!   - `InputEvent` maps committed insertions and simple backward/forward deletion intents.
//!   - DOM target ranges are node-relative and are not currently converted into `ui-events`
//!     replacement ranges.
//!
//! ## Example
//!
//! ```no_run
//! use web_sys::wasm_bindgen::JsCast;
//! use web_sys::{window, Event, KeyboardEvent};
//! use ui_events_web::{keyboard, pointer};
//!
//! // Inside an event listener closure…
//! # {
//! let ev: Event = /* from DOM */
//! # unimplemented!();
//! let win = window().unwrap();
//! let opts = pointer::Options::default()
//!     .with_scale(win.device_pixel_ratio())
//!     .with_coalesced(true)
//!     .with_predicted(true);
//!
//! if let Some(pe) = pointer::pointer_event_from_dom_event(&ev, &opts) {
//!     match pe {
//!         ui_events::pointer::PointerEvent::Move(update) => {
//!             // Use update.current; update.coalesced / update.predicted may be empty
//!         }
//!         ui_events::pointer::PointerEvent::Down(_) => {}
//!         ui_events::pointer::PointerEvent::Up(_) => {}
//!         _ => {}
//!     }
//! }
//!
//! if let Some(ke) = ev.dyn_ref::<KeyboardEvent>() {
//!     let k = keyboard::from_web_keyboard_event(ke);
//!     // Use k.state, k.code, k.key, k.modifiers …
//! }
//! # }
//! ```
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
#![no_std]

extern crate alloc;

pub mod keyboard;
pub mod pointer;
pub mod text;
