// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared `no_std` mapping helpers for Apple platform adapters.
//!
//! This crate owns platform-neutral construction of raw [`ui-events`] building
//! blocks from values extracted by the AppKit and UIKit adapter crates. It
//! intentionally does not depend on AppKit, UIKit, or `objc2`, and it does not
//! decide which platform callbacks should become which high-level events.
//!
//! ## Feature Policy
//!
//! - `std` is enabled by default.
//! - `libm` is accepted as a no-op compatibility feature for the workspace
//!   `no_std` check matrix.
//!
//! ## Helper Groups
//!
//! - Pointer identity helpers reserve [`PointerId::PRIMARY`] and map platform
//!   ids through [`PLATFORM_POINTER_ID_OFFSET`].
//! - Button helpers translate platform button indexes and bitmasks into
//!   [`PointerButton`] and [`PointerButtons`].
//! - Modifier helpers build [`Modifiers`] from platform modifier bits.
//! - State helpers build [`PointerState`] and [`ScrollDelta`] from finite raw
//!   values already extracted by an adapter crate.
//! - Text helpers build [`TextInputEvent`] values from UTF-16 ranges used by
//!   Apple text-input APIs.
//!
//! These helpers are intentionally small and value-based. AppKit and UIKit
//! crates still own all Objective-C selector access and platform-specific event
//! routing.
//!
//! [`Modifiers`]: ui_events::keyboard::Modifiers
//! [`PointerButton`]: ui_events::pointer::PointerButton
//! [`PointerButtons`]: ui_events::pointer::PointerButtons
//! [`PointerId::PRIMARY`]: ui_events::pointer::PointerId::PRIMARY
//! [`PointerState`]: ui_events::pointer::PointerState
//! [`ScrollDelta`]: ui_events::ScrollDelta
//! [`TextInputEvent`]: ui_events::text::TextInputEvent
//! [`ui-events`]: https://docs.rs/ui-events/

#![no_std]

extern crate alloc;

mod buttons;
mod identity;
mod modifiers;
mod state;
pub mod text;

pub use buttons::{buttons_from_bitmask, try_from_button_index};
pub use identity::{
    PLATFORM_POINTER_ID_OFFSET, pointer_info_from_platform_ids,
    pointer_info_from_platform_pointer_id, pointer_info_primary_for_type,
};
pub use modifiers::modifiers_from_bools;
pub use state::{pointer_scroll_delta_from_raw, pointer_state_from_raw};
