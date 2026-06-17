// Copyright 2025 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! # Pointer event types
//!
//! This module contains the core types for representing pointer input across
//! mice, pens, and touch. These types are transport-agnostic and can be
//! produced from platform backends (e.g. winit, web) and consumed in UI code.
//!
//! ## Key ideas:
//!
//! - [`PointerId`] and [`PersistentDeviceId`] help correlate states over time.
//! - [`PointerState`] carries position, pressure, tilt, modifiers and more.
//! - [`PointerEvent`] is the main event enum: down/up/move/enter/leave/scroll/gesture.
//! - [`PointerInfo::is_primary_pointer`] is a convenience for primary interactions.
//!
//! ## Example: checking for primary pointer and using logical coordinates
//!
//! ```
//! use ui_events::pointer::{PointerEvent, PointerUpdate};
//!
//! fn handle(ev: PointerEvent) {
//!     if let PointerEvent::Move(PointerUpdate { pointer, current, .. }) = ev {
//!         if pointer.is_primary_pointer() {
//!             let lp = current.logical_position();
//!             let _ = (lp.x, lp.y);
//!         }
//!     }
//! }
//! ```

mod buttons;

pub use buttons::{PointerButton, PointerButtons};

extern crate alloc;
use alloc::vec::Vec;

use core::num::NonZeroU64;

use dpi::{LogicalPosition, PhysicalPosition, PhysicalSize};
use keyboard_types::Modifiers;

use crate::ScrollDelta;

/// A unique identifier for the pointer.
///
/// PointerId(1) is reserved for the primary pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PointerId(NonZeroU64);

impl PointerId {
    /// The id of the primary pointer.
    pub const PRIMARY: Self = Self(NonZeroU64::MIN);

    /// Make a new `PointerId` from a `u64`.
    #[inline(always)]
    pub fn new(n: u64) -> Option<Self> {
        NonZeroU64::new(n).map(PointerId)
    }

    /// Return `true` if this is the primary `PointerId`.
    #[inline(always)]
    pub fn is_primary_pointer(self) -> bool {
        self == Self::PRIMARY
    }

    /// Returns the inner `NonZeroU64` value.
    #[inline(always)]
    pub fn get_inner(self) -> NonZeroU64 {
        self.0
    }
}

/// An identifier for the pointing device that is stable across the session.
///
/// PointerId(1) is reserved for the primary pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PersistentDeviceId(NonZeroU64);

impl PersistentDeviceId {
    /// Make a new `PersistentDeviceId` from a `u64`.
    #[inline(always)]
    pub fn new(n: u64) -> Option<Self> {
        NonZeroU64::new(n).map(PersistentDeviceId)
    }
}

/// The type of device that has generated a pointer event.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum PointerType {
    /// The type of device could not be determined.
    #[default]
    Unknown,
    /// A mouse.
    Mouse,
    /// A pen or stylus.
    Pen,
    /// A touch contact.
    Touch,
}

/// Identifying information about a pointer, stable across states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PointerInfo {
    /// Pointer ID.
    ///
    /// [`PointerId::PRIMARY`] is reserved for the primary pointer,
    /// so when converting platform pointer IDs on a platform that
    /// does not reserve this value, add an offset to avoid collision.
    ///
    /// `None` is for events not originating from a pointing device.
    pub pointer_id: Option<PointerId>,
    /// Persistent device ID.
    ///
    /// This should be set when the platform can identify a given pointing
    /// device during the whole session, and associate it with events.
    /// If this is not possible for the given event, it should be `None`.
    pub persistent_device_id: Option<PersistentDeviceId>,
    /// Pointer type.
    pub pointer_type: PointerType,
}

impl PointerInfo {
    /// Returns `true` if this is the primary pointer.
    #[inline(always)]
    pub fn is_primary_pointer(&self) -> bool {
        self.pointer_id.is_some_and(PointerId::is_primary_pointer)
    }
}

/// Orientation of a pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerOrientation {
    /// Spherical altitude.
    ///
    /// 0 is parallel to the surface, π/2 is perpendicular.
    pub altitude: f32,
    /// Spherical azimuth.
    ///
    /// 0 is the positive x axis, π/2 is positive y.
    pub azimuth: f32,
}

impl Default for PointerOrientation {
    fn default() -> Self {
        Self {
            altitude: core::f32::consts::FRAC_PI_2,
            azimuth: core::f32::consts::FRAC_PI_2,
        }
    }
}

/// The size of an input, usually touch.
///
/// If this is not provided by the underlying API, platform, or device,
/// then it should be a single pixel.
pub type ContactGeometry = PhysicalSize<f64>;

/// A single pointer state.
#[derive(Clone, Debug, PartialEq)]
pub struct PointerState {
    /// Monotonic nanoseconds in the event stream's clock domain.
    ///
    /// The epoch is intentionally unspecified: it may be the host's frame
    /// clock, a platform monotonic clock, or an adapter-local monotonic clock.
    /// Values are meaningful for ordering and relative timing within one event
    /// stream, and all states in a single event, including coalesced and
    /// predicted states, should use the same clock domain.
    pub time: u64,
    /// Position.
    pub position: PhysicalPosition<f64>,
    /// Pressed buttons.
    pub buttons: PointerButtons,
    /// Modifiers state.
    pub modifiers: Modifiers,
    /// Click or tap count associated with the pointer.
    pub count: u8,
    /// The size of an input, usually touch.
    ///
    /// If this is not provided by the underlying API, platform, or device,
    /// then it should be a single pixel.
    pub contact_geometry: ContactGeometry,
    /// Orientation.
    pub orientation: PointerOrientation,
    /// Normalized pressure in range 0..1.
    ///
    /// Where pressure is not reported by the platform, it
    /// is always 0.5 when activated and 0.0 when not.
    pub pressure: f32,
    /// Normalized ‘tangential pressure’ in range -1..1.
    ///
    /// This is an arbitrary parameter and, despite its name,
    /// it may not originate from a pressure sensitive control.
    /// This is often controlled by something like a wheel on the
    /// barrel of an ‘airbrush’ style pen.
    pub tangential_pressure: f32,
    /// The scale factor of the window/screen where this pointer event occurred.
    pub scale_factor: f64,
}

impl PointerState {
    /// Returns the pointer position as a logical `kurbo::Point`.
    ///
    /// The position is converted from physical pixels to logical coordinates
    /// using the state's scale factor.
    #[cfg(feature = "kurbo")]
    pub fn logical_point(&self) -> kurbo::Point {
        let log = self.position.to_logical(self.scale_factor);
        kurbo::Point { x: log.x, y: log.y }
    }

    /// Returns the pointer position as a physical `kurbo::Point`.
    #[cfg(feature = "kurbo")]
    pub fn physical_point(&self) -> kurbo::Point {
        kurbo::Point {
            x: self.position.x,
            y: self.position.y,
        }
    }

    /// Returns the pointer position in logical coordinates.
    ///
    /// This converts the physical position to logical coordinates using
    /// the state's scale factor, providing DPI-independent positioning.
    pub fn logical_position(&self) -> LogicalPosition<f64> {
        self.position.to_logical(self.scale_factor)
    }
}

impl Default for PointerState {
    fn default() -> Self {
        Self {
            time: 0,
            position: PhysicalPosition::<f64>::default(),
            buttons: PointerButtons::default(),
            modifiers: Modifiers::default(),
            count: 0,
            contact_geometry: ContactGeometry {
                width: 1.0,
                height: 1.0,
            },
            orientation: PointerOrientation::default(),
            // No buttons pressed, therefore no pressure.
            pressure: 0.0,
            tangential_pressure: 0.0,
            scale_factor: 1.,
        }
    }
}

/// A pointer update, along with coalesced and predicted states.
#[derive(Clone, Debug, PartialEq)]
pub struct PointerUpdate {
    /// Identifying information about pointer.
    pub pointer: PointerInfo,
    /// Current state.
    pub current: PointerState,
    /// Coalesced states, ordered by `time`.
    ///
    /// Coalescing is application-specific.
    /// On the web, the browser does its own coalescing, whereas
    /// on other platforms you may do your own, or forego it
    /// altogether, delivering every state.
    pub coalesced: Vec<PointerState>,
    /// Predicted states, ordered by `time`.
    ///
    /// Some platforms provide predicted states directly,
    /// and you may choose to add your own predictor.
    pub predicted: Vec<PointerState>,
}

impl PointerUpdate {
    /// Returns `true` if this is the primary pointer.
    #[inline(always)]
    pub fn is_primary_pointer(&self) -> bool {
        self.pointer.is_primary_pointer()
    }
}

/// An event representing a [`PointerButton`] that was pressed or released.
#[derive(Clone, Debug)]
pub struct PointerButtonEvent {
    /// The [`PointerButton`] that was pressed.
    pub button: Option<PointerButton>,
    /// Identity of the pointer.
    pub pointer: PointerInfo,
    /// The state of the pointer (i.e. position, pressure, etc.).
    pub state: PointerState,
}

/// An event representing a scroll
#[derive(Clone, Debug)]
pub struct PointerScrollEvent {
    /// Identity of the pointer.
    pub pointer: PointerInfo,
    /// The delta of the scroll.
    pub delta: ScrollDelta,
    /// The state of the pointer (i.e. position, pressure, etc.).
    pub state: PointerState,
}

/// A touchpad gesture for pointer.
#[derive(Clone, Debug)]
pub enum PointerGesture {
    /// Pinch delta.
    ///
    /// This is a signed quantity as a fraction of the current scale.
    ///
    /// For example, `0.1` means “increase scale by 10%”, and `-0.1` means
    /// “decrease scale by 10%”.
    ///
    /// A common update rule is `new_scale = current_scale * (1.0 + delta)`.
    Pinch(f32),
    /// Clockwise rotation in radians.
    ///
    /// This is a delta for this update, not an absolute angle.
    Rotate(f32),
}

/// An event representing a gesture
#[derive(Clone, Debug)]
pub struct PointerGestureEvent {
    /// Identity of the pointer.
    pub pointer: PointerInfo,
    /// The gesture being performed.
    pub gesture: PointerGesture,
    /// The state of the pointer (i.e. position, pressure, etc.).
    pub state: PointerState,
}

/// A standard `PointerEvent`.
///
/// This is intentionally limited to standard pointer events,
/// and it is expected that applications and frameworks that
/// support more event types will use this as a base and add
/// what they need in a conversion.
#[derive(Clone, Debug)]
pub enum PointerEvent {
    /// A [`PointerButton`] was pressed.
    Down(PointerButtonEvent),
    /// A [`PointerButton`] was released.
    Up(PointerButtonEvent),
    /// Pointer moved.
    Move(PointerUpdate),
    /// Pointer motion was cancelled.
    ///
    /// Usually this is a touch which was taken over somewhere else.
    /// You should try to undo the effect of the gesture when you receive this.
    Cancel(PointerInfo),
    /// Pointer entered the area that receives this event.
    Enter(PointerInfo),
    /// Pointer left the area that receives these events.
    Leave(PointerInfo),
    /// A scroll was requested at the pointer location.
    ///
    /// Usually this is caused by a mouse wheel or a touchpad.
    Scroll(PointerScrollEvent),
    /// Gesture at pointer.
    Gesture(PointerGestureEvent),
}

impl PointerEvent {
    /// Returns `true` if this event is for the primary pointer.
    #[inline(always)]
    pub fn is_primary_pointer(&self) -> bool {
        match self {
            Self::Down(PointerButtonEvent { pointer, .. })
            | Self::Up(PointerButtonEvent { pointer, .. })
            | Self::Move(PointerUpdate { pointer, .. })
            | Self::Cancel(pointer)
            | Self::Enter(pointer)
            | Self::Leave(pointer)
            | Self::Scroll(PointerScrollEvent { pointer, .. })
            | Self::Gesture(PointerGestureEvent { pointer, .. }) => pointer.is_primary_pointer(),
        }
    }
}
