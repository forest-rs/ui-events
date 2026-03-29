// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UIKit (iOS/tvOS) shims that extract fields from `objc2_ui_kit` and forward
//! to platform-neutral mappers.

// cfg is handled at the module declaration in lib.rs

use alloc::string::ToString;
use objc2_foundation::NSArray;
use objc2_ui_kit::{UIEvent, UITouch, UITouchPhase, UITouchType};
#[cfg(feature = "gestures")]
use objc2_ui_kit::{
    UIGestureRecognizerState, UIPinchGestureRecognizer, UIRotationGestureRecognizer,
};
use objc2_ui_kit::{UIKey, UIPress, UIPressPhase, UIPressType};

use crate::mapping as map;
use ui_events::keyboard::Modifiers;
use ui_events::keyboard::{Code, Key, KeyState, KeyboardEvent, Location, NamedKey};
use ui_events::pointer::{PointerEvent, PointerState, PointerType};
#[cfg(feature = "gestures")]
use ui_events::pointer::{PointerGesture, PointerGestureEvent};
use ui_events::pointer::{PointerInfo, PointerUpdate};

fn pointer_type_from_touch(touch: &UITouch) -> PointerType {
    match touch.r#type() {
        UITouchType::Pencil => PointerType::Pen,
        _ => PointerType::Touch,
    }
}

fn pointer_state_from_touch(
    touch: &UITouch,
    scale_factor: f64,
    pointer_type: PointerType,
    event_buttons_mask: u64,
) -> PointerState {
    let loc = touch.preciseLocationInView(None);
    let ts = touch.timestamp();
    let phase = touch.phase();

    let active = matches!(
        phase,
        UITouchPhase::Began | UITouchPhase::Moved | UITouchPhase::Stationary
    );
    let buttons_mask =
        map::contact_buttons_mask_for_uikit_pointer(pointer_type, active, event_buttons_mask);

    let force_along_axis = touch.force();
    let max_force = touch.maximumPossibleForce();
    let tap_count = i64::try_from(touch.tapCount()).unwrap_or(i64::MAX);
    let contact_diameter = Some(touch.majorRadius() * 2.0);

    let (altitude, azimuth) = if matches!(pointer_type, PointerType::Pen) {
        let altitude = touch.altitudeAngle();
        let azimuth = touch.azimuthAngleInView(None);
        (Some(altitude), Some(azimuth))
    } else {
        (None, None)
    };

    let pressure = map::pressure_from_force(
        active,
        Some(force_along_axis),
        Some(max_force),
        altitude,
        matches!(pointer_type, PointerType::Pen),
    );

    let state = map::pointer_state_from_raw(
        loc.x,
        loc.y,
        scale_factor,
        buttons_mask,
        false,
        false,
        false,
        false,
        Some(pressure),
        // UIKit exposes stylus rollAngle, not tangential pressure.
        None,
        contact_diameter,
        contact_diameter,
        ts,
        tap_count,
    );

    #[allow(
        clippy::cast_possible_truncation,
        reason = "UIKit provides f64 radians; ui-events stores orientation as f32"
    )]
    if let (Some(alt), Some(azi)) = (altitude, azimuth) {
        ui_events::pointer::PointerState {
            orientation: ui_events::pointer::PointerOrientation {
                altitude: alt as f32,
                azimuth: azi as f32,
            },
            ..state
        }
    } else {
        state
    }
}

fn event_buttons_mask(event: Option<&UIEvent>) -> u64 {
    event
        .and_then(|event| u64::try_from(event.buttonMask().bits()).ok())
        .unwrap_or_default()
}

fn pointer_info_from_touch(touch: &UITouch) -> PointerInfo {
    let pointer_token = u64::try_from(touch as *const UITouch as usize)
        .expect("object pointers always fit into u64");
    map::pointer_info_from_platform_pointer_id(pointer_type_from_touch(touch), pointer_token)
}

/// Convert a `UITouch` + `UIEvent` into a `PointerEvent`.
///
/// This is the preferred entry point when handling `touchesBegan/Moved/Ended/Cancelled`,
/// since `UIEvent` is where UIKit exposes coalesced and predicted touch samples.
///
/// Positions come from `UITouch::preciseLocationInView(None)`, so they use
/// UIKit's coordinate conventions for the touch's window. This helper only
/// scales point values into physical pixels; hosts that need another view's
/// coordinate space should convert before or after calling it.
///
/// Touch timestamps are UIKit's monotonic seconds-since-boot timebase, not Unix
/// epoch time.
///
/// Pencil hover is also handled here when UIKit reports `RegionEntered`, `RegionMoved`,
/// and `RegionExited` touch phases.
pub fn pointer_event_from_touch_and_event(
    touch: &UITouch,
    event: &UIEvent,
    scale_factor: f64,
) -> Option<PointerEvent> {
    let phase = touch.phase();
    let pointer_type = pointer_type_from_touch(touch);
    let pointer = pointer_info_from_touch(touch);
    let event_buttons_mask = event_buttons_mask(Some(event));

    // `coalescedTouchesForTouch:` is most useful for move-like phases where we can fill
    // `PointerUpdate.coalesced`; for other phases we just return the single-sample event.
    if !matches!(phase, UITouchPhase::Moved | UITouchPhase::Stationary) {
        return pointer_event_from_touch_impl(touch, scale_factor, event_buttons_mask);
    }

    let mut predicted = alloc::vec![];
    if let Some(arr) = event.predictedTouchesForTouch(touch) {
        predicted =
            pointer_states_from_nsarray(&arr, scale_factor, pointer_type, event_buttons_mask);
    }

    let Some(arr) = event.coalescedTouchesForTouch(touch) else {
        return Some(PointerEvent::Move(PointerUpdate {
            pointer,
            current: pointer_state_from_touch(
                touch,
                scale_factor,
                pointer_type,
                event_buttons_mask,
            ),
            coalesced: alloc::vec![],
            predicted,
        }));
    };

    let mut samples =
        pointer_states_from_nsarray(&arr, scale_factor, pointer_type, event_buttons_mask);
    let current = samples.pop().unwrap_or_else(|| {
        pointer_state_from_touch(touch, scale_factor, pointer_type, event_buttons_mask)
    });

    Some(PointerEvent::Move(PointerUpdate {
        pointer,
        current,
        coalesced: samples,
        predicted,
    }))
}

fn pointer_states_from_nsarray(
    arr: &NSArray<UITouch>,
    scale_factor: f64,
    pointer_type: PointerType,
    event_buttons_mask: u64,
) -> alloc::vec::Vec<PointerState> {
    let mut out = alloc::vec![];
    let n = arr.count();
    out.reserve(n);
    for i in 0..n {
        let sample = arr.objectAtIndex(i);
        out.push(pointer_state_from_touch(
            &sample,
            scale_factor,
            pointer_type,
            event_buttons_mask,
        ));
    }
    out
}

/// Convert a `UITouch` into a `PointerEvent`.
///
/// This is an uncommon convenience entry point for situations where you do not have
/// access to the associated `UIEvent`. Without `UIEvent`, UIKit does not expose
/// coalesced or predicted touch samples, so `PointerUpdate.coalesced/predicted` will be empty.
///
/// Coordinates and timestamps follow the same conventions as
/// [`pointer_event_from_touch_and_event`].
///
/// Pencil hover is also handled here when UIKit reports `RegionEntered`, `RegionMoved`,
/// and `RegionExited` touch phases.
pub fn pointer_event_from_touch(touch: &UITouch, scale_factor: f64) -> Option<PointerEvent> {
    pointer_event_from_touch_impl(touch, scale_factor, 0)
}

fn pointer_event_from_touch_impl(
    touch: &UITouch,
    scale_factor: f64,
    event_buttons_mask: u64,
) -> Option<PointerEvent> {
    let phase = touch.phase();
    let pointer_type = pointer_type_from_touch(touch);
    let pointer = pointer_info_from_touch(touch);
    let state = pointer_state_from_touch(touch, scale_factor, pointer_type, event_buttons_mask);
    let button = map::contact_button_for_uikit_pointer(pointer_type);

    Some(match phase {
        UITouchPhase::Began => PointerEvent::Down(ui_events::pointer::PointerButtonEvent {
            button,
            pointer,
            state,
        }),
        UITouchPhase::Moved | UITouchPhase::Stationary => PointerEvent::Move(PointerUpdate {
            pointer,
            current: state,
            coalesced: alloc::vec![],
            predicted: alloc::vec![],
        }),
        UITouchPhase::Ended => PointerEvent::Up(ui_events::pointer::PointerButtonEvent {
            button,
            pointer,
            state,
        }),
        UITouchPhase::RegionEntered if matches!(pointer_type, PointerType::Pen) => {
            PointerEvent::Enter(pointer)
        }
        UITouchPhase::RegionMoved if matches!(pointer_type, PointerType::Pen) => {
            PointerEvent::Move(PointerUpdate {
                pointer,
                current: state,
                coalesced: alloc::vec![],
                predicted: alloc::vec![],
            })
        }
        UITouchPhase::RegionExited if matches!(pointer_type, PointerType::Pen) => {
            PointerEvent::Leave(pointer)
        }
        UITouchPhase::Cancelled => PointerEvent::Cancel(pointer),
        _ => return None,
    })
}

/// Convert a `UIPinchGestureRecognizer` into a `PointerEvent::Gesture`.
///
/// This is a lightweight convenience helper intended to be called from a
/// `UIPinchGestureRecognizer` callback.
///
/// Notes:
/// - Returns `None` unless the recognizer is in `Began` or `Changed` state.
/// - `previous_scale` must be the previous cumulative `gesture.scale()` value,
///   or `1.0` for the first update in a gesture.
/// - UIKit reports `scale` as a cumulative multiplicative factor; this helper
///   differences it against `previous_scale` so the returned
///   [`PointerGesture::Pinch`] satisfies the `ui-events` per-update delta
///   contract.
/// - The returned [`PointerState::time`] is 0, since gesture recognizers do not expose a timestamp.
#[cfg(feature = "gestures")]
pub fn pointer_gesture_from_uipinch(
    gesture: &UIPinchGestureRecognizer,
    scale_factor: f64,
    previous_scale: f64,
) -> Option<PointerEvent> {
    let gr_state: UIGestureRecognizerState = gesture.state();
    if !(gr_state == UIGestureRecognizerState::Began
        || gr_state == UIGestureRecognizerState::Changed)
    {
        return None;
    }
    let loc = gesture.locationInView(None);
    let scale = gesture.scale();
    let pinch = map::pinch_delta_from_cumulative_scale(previous_scale, scale)?;
    let state = map::pointer_state_from_raw(
        loc.x,
        loc.y,
        scale_factor,
        0,
        false,
        false,
        false,
        false,
        None,
        None,
        None,
        None,
        0.0,
        0,
    );
    Some(PointerEvent::Gesture(PointerGestureEvent {
        pointer: map::pointer_info_primary_for_type(PointerType::Touch),
        gesture: PointerGesture::Pinch(pinch),
        state,
    }))
}

/// Convert a `UIRotationGestureRecognizer` into a `PointerEvent::Gesture`.
///
/// This is a lightweight convenience helper intended to be called from a
/// `UIRotationGestureRecognizer` callback.
///
/// Notes:
/// - Returns `None` unless the recognizer is in `Began` or `Changed` state.
/// - `previous_rotation_ccw` must be the previous cumulative
///   `gesture.rotation()` value, or `0.0` for the first update in a gesture.
/// - UIKit reports rotation as cumulative counterclockwise radians; this helper
///   differences it against `previous_rotation_ccw` and flips the sign so the
///   returned [`PointerGesture::Rotate`] satisfies the `ui-events`
///   clockwise-per-update delta contract.
/// - The returned [`PointerState::time`] is 0, since gesture recognizers do not expose a timestamp.
#[cfg(feature = "gestures")]
pub fn pointer_gesture_from_uirotation(
    gesture: &UIRotationGestureRecognizer,
    scale_factor: f64,
    previous_rotation_ccw: f64,
) -> Option<PointerEvent> {
    let gr_state: UIGestureRecognizerState = gesture.state();
    if !(gr_state == UIGestureRecognizerState::Began
        || gr_state == UIGestureRecognizerState::Changed)
    {
        return None;
    }
    let loc = gesture.locationInView(None);
    let rotation_ccw = gesture.rotation();
    let rotation_cw =
        map::rotation_delta_from_cumulative_rotation(previous_rotation_ccw, rotation_ccw)?;
    let state = map::pointer_state_from_raw(
        loc.x,
        loc.y,
        scale_factor,
        0,
        false,
        false,
        false,
        false,
        None,
        None,
        None,
        None,
        0.0,
        0,
    );
    Some(PointerEvent::Gesture(PointerGestureEvent {
        pointer: map::pointer_info_primary_for_type(PointerType::Touch),
        gesture: PointerGesture::Rotate(rotation_cw),
        state,
    }))
}

/// Convert a `UIPress` (tvOS/Apple TV remote) into a `KeyboardEvent`.
pub fn keyboard_event_from_uipress(press: &UIPress) -> Option<KeyboardEvent> {
    let state = match press.phase() {
        UIPressPhase::Began => KeyState::Down,
        UIPressPhase::Ended | UIPressPhase::Cancelled => KeyState::Up,
        _ => return None,
    };
    let key = match press.r#type() {
        UIPressType::UpArrow => Key::Named(NamedKey::ArrowUp),
        UIPressType::DownArrow => Key::Named(NamedKey::ArrowDown),
        UIPressType::LeftArrow => Key::Named(NamedKey::ArrowLeft),
        UIPressType::RightArrow => Key::Named(NamedKey::ArrowRight),
        UIPressType::Select => Key::Named(NamedKey::Select),
        UIPressType::Menu => Key::Named(NamedKey::Escape),
        UIPressType::PlayPause => Key::Named(NamedKey::MediaPlayPause),
        _ => Key::Named(NamedKey::Unidentified),
    };
    Some(KeyboardEvent {
        state,
        key,
        code: Code::Unidentified,
        location: Location::Standard,
        modifiers: Modifiers::default(),
        is_composing: false,
        repeat: false,
    })
}

/// Convert a `UIPress` that carries a `UIKey` (hardware keyboard) into a `KeyboardEvent`.
pub fn keyboard_event_from_uikey(press: &UIPress, key: &UIKey) -> Option<KeyboardEvent> {
    let state = match press.phase() {
        UIPressPhase::Began => KeyState::Down,
        UIPressPhase::Ended | UIPressPhase::Cancelled => KeyState::Up,
        _ => return None,
    };

    let usage = u16::try_from(key.keyCode().0).ok();
    let (code, named, location) = usage.map(map::hid_usage_to_code_named_location).unwrap_or((
        Code::Unidentified,
        Some(NamedKey::Unidentified),
        Location::Standard,
    ));
    let modifier_bits = u64::try_from(key.modifierFlags().bits()).unwrap_or_default();
    let modifiers = map::modifiers_from_uikit_modifier_bits(modifier_bits);

    let characters = key.characters().to_string();
    let key_value = if let Some(named) = named {
        Key::Named(named)
    } else if characters.chars().count() == 1 {
        Key::Character(characters)
    } else {
        let unmodified = key.charactersIgnoringModifiers().to_string();
        if unmodified.chars().count() == 1 {
            Key::Character(unmodified)
        } else {
            Key::Named(NamedKey::Unidentified)
        }
    };

    Some(KeyboardEvent {
        state,
        key: key_value,
        code,
        location,
        modifiers,
        is_composing: false,
        repeat: false,
    })
}
