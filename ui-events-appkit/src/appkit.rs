// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! `AppKit` (macOS) shims that extract fields from `objc2_app_kit::NSEvent` and
//! forward to platform-neutral mappers.

// cfg is handled at the module declaration in lib.rs

use alloc::string::ToString;
use objc2_app_kit::{
    NSEvent, NSEventModifierFlags, NSEventSubtype, NSEventType, NSPointingDeviceType,
};

use crate::mapping as map;
use ui_events::keyboard::{Code, Key, KeyState, KeyboardEvent, Location, NamedKey};
use ui_events::pointer::{
    PointerButtonEvent, PointerEvent, PointerGesture, PointerGestureEvent, PointerScrollEvent,
    PointerType, PointerUpdate,
};

mod keycodes;
use keycodes as vk;

fn mods_has(flags: NSEventModifierFlags, bit: NSEventModifierFlags) -> bool {
    flags.contains(bit)
}

fn pointer_type_from_pointing_device(pointing_device: NSPointingDeviceType) -> PointerType {
    match pointing_device {
        NSPointingDeviceType::Pen | NSPointingDeviceType::Eraser => PointerType::Pen,
        _ => PointerType::Mouse,
    }
}

fn pen_pointer_info_for_mouse_event(
    subtype: NSEventSubtype,
    pointing_device: NSPointingDeviceType,
    unique_id: u64,
    device_id: u64,
) -> Option<ui_events::pointer::PointerInfo> {
    (subtype == NSEventSubtype::TabletPoint
        && matches!(
            pointing_device,
            NSPointingDeviceType::Pen | NSPointingDeviceType::Eraser
        ))
    .then(|| {
        map::pointer_info_from_platform_ids(PointerType::Pen, Some(unique_id), Some(device_id))
    })
}

fn pen_buttons_mask(pointing_device: NSPointingDeviceType, buttons_mask: u64) -> u64 {
    if pointing_device == NSPointingDeviceType::Eraser {
        buttons_mask | (1_u64 << 5)
    } else {
        buttons_mask
    }
}

fn pen_transition_button(
    pointing_device: NSPointingDeviceType,
    button_number: i64,
) -> Option<ui_events::pointer::PointerButton> {
    if pointing_device == NSPointingDeviceType::Eraser {
        Some(ui_events::pointer::PointerButton::PenEraser)
    } else {
        map::try_from_button_index(button_number)
    }
}

fn button_mask_bit(button_number: i64) -> Option<u64> {
    (0..32)
        .contains(&button_number)
        .then(|| 1_u64 << button_number)
}

fn mouse_buttons_after_down(current_mask: u64, button_number: i64) -> u64 {
    button_mask_bit(button_number).map_or(current_mask, |bit| current_mask | bit)
}

fn mouse_buttons_after_up(current_mask: u64, button_number: i64) -> u64 {
    button_mask_bit(button_number).map_or(current_mask, |bit| current_mask & !bit)
}

fn click_count_for_mouse_button_event(
    event_type: NSEventType,
    read_click_count: impl FnOnce() -> i64,
) -> i64 {
    match event_type {
        NSEventType::LeftMouseDown
        | NSEventType::RightMouseDown
        | NSEventType::OtherMouseDown
        | NSEventType::LeftMouseUp
        | NSEventType::RightMouseUp
        | NSEventType::OtherMouseUp => read_click_count(),
        _ => 0,
    }
}

fn pen_pointer_info_for_mouse_nsevent(
    event_type: NSEventType,
    read_pen_pointer_info: impl FnOnce() -> Option<ui_events::pointer::PointerInfo>,
) -> Option<ui_events::pointer::PointerInfo> {
    match event_type {
        NSEventType::LeftMouseDown
        | NSEventType::RightMouseDown
        | NSEventType::OtherMouseDown
        | NSEventType::LeftMouseUp
        | NSEventType::RightMouseUp
        | NSEventType::OtherMouseUp
        | NSEventType::MouseMoved
        | NSEventType::LeftMouseDragged
        | NSEventType::RightMouseDragged
        | NSEventType::OtherMouseDragged => read_pen_pointer_info(),
        _ => None,
    }
}

/// Convert an AppKit keyboard `NSEvent` into a `ui-events` [`KeyboardEvent`].
pub fn keyboard_event_from_nsevent(e: &NSEvent) -> Option<KeyboardEvent> {
    let ty = e.r#type();
    if ty == NSEventType::FlagsChanged {
        // Modifier toggle treated as a KeyDown/KeyUp on the modifier.
        let (code, named, location, flag) = map_modifier_toggle(e.keyCode())?;
        let flags = e.modifierFlags();
        // Known limitation: `modifierFlags` exposes aggregate modifier state.
        // If both left and right variants of the same modifier are held,
        // releasing one side still leaves the aggregate bit set and is reported
        // as down. Distinguishing that requires device-dependent masks.
        let is_down = flags.contains(flag);
        return Some(ui_events::keyboard::KeyboardEvent {
            state: if is_down {
                KeyState::Down
            } else {
                KeyState::Up
            },
            key: Key::Named(named),
            code,
            location,
            modifiers: map::modifiers_from_bools(
                mods_has(flags, NSEventModifierFlags::Control),
                mods_has(flags, NSEventModifierFlags::Option),
                mods_has(flags, NSEventModifierFlags::Shift),
                mods_has(flags, NSEventModifierFlags::Command),
            ),
            is_composing: false,
            repeat: false,
        });
    }

    let is_down = match ty {
        NSEventType::KeyDown => true,
        NSEventType::KeyUp => false,
        _ => return None,
    };
    let flags = e.modifierFlags();

    let ctrl = mods_has(flags, NSEventModifierFlags::Control);
    let alt = mods_has(flags, NSEventModifierFlags::Option);
    let shift = mods_has(flags, NSEventModifierFlags::Shift);
    let meta = mods_has(flags, NSEventModifierFlags::Command);

    let (code, named, location) = map_virtual_keycode_to_code_named_location(e.keyCode());
    let characters = e.characters().map(|chars| chars.to_string());
    let unmodified_characters = e
        .charactersIgnoringModifiers()
        .map(|chars| chars.to_string());
    let key_value = key_from_appkit_strings(
        named,
        characters.as_deref(),
        unmodified_characters.as_deref(),
    );

    Some(KeyboardEvent {
        state: if is_down {
            KeyState::Down
        } else {
            KeyState::Up
        },
        key: key_value,
        code,
        location,
        modifiers: map::modifiers_from_bools(ctrl, alt, shift, meta),
        is_composing: false,
        repeat: e.isARepeat(),
    })
}

/// Convert an AppKit pointer-related `NSEvent` into a `ui-events` [`PointerEvent`].
///
/// Positions come from [`NSEvent::locationInWindow`], so they are in AppKit
/// window coordinates with AppKit's origin convention. This helper only scales
/// point values into physical pixels; hosts that need view-local or flipped
/// coordinates should call [`pointer_event_from_nsevent_at_position`] instead.
///
/// Event timestamps are AppKit's monotonic seconds-since-boot timebase, not
/// Unix epoch time.
///
/// Mouse button down/up click counts use AppKit's `clickCount` directly.
/// Other pointer event kinds, including scroll events, use a count of `0`
/// without querying click metadata from AppKit.
///
/// Mouse events may query AppKit tablet metadata when AppKit reports tablet
/// mouse-event subtypes. Scroll events do not query tablet metadata.
///
/// Scroll deltas preserve AppKit's `scrollingDeltaX/Y` sign. AppKit already
/// applies the user's natural-scrolling preference to those values, and this
/// matches the macOS path used by `winit`.
///
/// Magnify gesture events map to [`PointerGesture::Pinch`] using AppKit's
/// per-event `magnification` delta.
pub fn pointer_event_from_nsevent(e: &NSEvent, scale_factor: f64) -> Option<PointerEvent> {
    let p = e.locationInWindow();
    pointer_event_from_nsevent_at_position(e, scale_factor, p.x, p.y)
}

/// Convert an AppKit pointer-related `NSEvent` at a caller-provided position.
///
/// `x_points` and `y_points` are logical AppKit points in the coordinate space
/// chosen by the caller. The values are scaled into physical pixels by
/// `scale_factor`. Use this when an `NSEvent` has already been converted from
/// window coordinates into a specific `NSView` or other host coordinate space.
///
/// Event timestamps are AppKit's monotonic seconds-since-boot timebase, not
/// Unix epoch time.
///
/// Mouse button down/up click counts use AppKit's `clickCount` directly.
/// Other pointer event kinds, including scroll events, use a count of `0`
/// without querying click metadata from AppKit.
///
/// Mouse events may query AppKit tablet metadata when AppKit reports tablet
/// mouse-event subtypes. Scroll events do not query tablet metadata.
///
/// Scroll deltas preserve AppKit's `scrollingDeltaX/Y` sign. AppKit already
/// applies the user's natural-scrolling preference to those values, and this
/// matches the macOS path used by `winit`.
///
/// Magnify gesture events map to [`PointerGesture::Pinch`] using AppKit's
/// per-event `magnification` delta.
pub fn pointer_event_from_nsevent_at_position(
    e: &NSEvent,
    scale_factor: f64,
    x_points: f64,
    y_points: f64,
) -> Option<PointerEvent> {
    let ty = e.r#type();

    let flags = e.modifierFlags();

    let ctrl = mods_has(flags, NSEventModifierFlags::Control);
    let alt = mods_has(flags, NSEventModifierFlags::Option);
    let shift = mods_has(flags, NSEventModifierFlags::Shift);
    let meta = mods_has(flags, NSEventModifierFlags::Command);

    let timestamp_secs = e.timestamp();
    let click_count = click_count_for_mouse_button_event(ty, || e.clickCount() as i64);

    let pen_pointer_info = pen_pointer_info_for_mouse_nsevent(ty, || {
        pen_pointer_info_for_mouse_event(
            e.subtype(),
            e.pointingDeviceType(),
            e.uniqueID(),
            e.deviceID() as u64,
        )
    });

    let state = |buttons_mask: u64, pressure: Option<f64>, tangential_pressure: Option<f64>| {
        map::pointer_state_from_raw(
            x_points,
            y_points,
            scale_factor,
            buttons_mask,
            ctrl,
            alt,
            shift,
            meta,
            pressure,
            tangential_pressure,
            None,
            None,
            timestamp_secs,
            click_count,
        )
    };

    Some(match ty {
        NSEventType::LeftMouseDown | NSEventType::RightMouseDown | NSEventType::OtherMouseDown => {
            if let Some(pointer) = pen_pointer_info {
                let pointing_device = e.pointingDeviceType();
                let mut state = state(
                    pen_buttons_mask(pointing_device, e.buttonMask().0 as u64),
                    Some(e.pressure() as f64),
                    Some(e.tangentialPressure() as f64),
                );
                let tilt = e.tilt();
                state.orientation = map::orientation_from_tilt_fraction(tilt.x, tilt.y);
                PointerEvent::Down(ui_events::pointer::PointerButtonEvent {
                    button: pen_transition_button(pointing_device, e.buttonNumber() as i64),
                    pointer,
                    state,
                })
            } else {
                // `pressedMouseButtons` is AppKit's current global button
                // state for this callback. Force the transition button into
                // the state so this remains correct if AppKit's global mask is
                // observed before it includes the down transition.
                let buttons_mask = mouse_buttons_after_down(
                    NSEvent::pressedMouseButtons() as u64,
                    e.buttonNumber() as i64,
                );
                PointerEvent::Down(PointerButtonEvent {
                    button: map::try_from_button_index(e.buttonNumber() as i64),
                    pointer: map::pointer_info_primary_for_type(PointerType::Mouse),
                    state: state(buttons_mask, None, None),
                })
            }
        }
        NSEventType::LeftMouseUp | NSEventType::RightMouseUp | NSEventType::OtherMouseUp => {
            if let Some(pointer) = pen_pointer_info {
                let pointing_device = e.pointingDeviceType();
                let mut state = state(
                    pen_buttons_mask(pointing_device, e.buttonMask().0 as u64),
                    Some(e.pressure() as f64),
                    Some(e.tangentialPressure() as f64),
                );
                let tilt = e.tilt();
                state.orientation = map::orientation_from_tilt_fraction(tilt.x, tilt.y);
                PointerEvent::Up(ui_events::pointer::PointerButtonEvent {
                    button: pen_transition_button(pointing_device, e.buttonNumber() as i64),
                    pointer,
                    state,
                })
            } else {
                // `pressedMouseButtons` is AppKit's current global button
                // state for this callback. Force the transition button out of
                // the state so this remains correct if AppKit's global mask is
                // observed before it drops the up transition.
                let buttons_mask = mouse_buttons_after_up(
                    NSEvent::pressedMouseButtons() as u64,
                    e.buttonNumber() as i64,
                );
                PointerEvent::Up(PointerButtonEvent {
                    button: map::try_from_button_index(e.buttonNumber() as i64),
                    pointer: map::pointer_info_primary_for_type(PointerType::Mouse),
                    state: state(buttons_mask, None, None),
                })
            }
        }
        NSEventType::MouseMoved
        | NSEventType::LeftMouseDragged
        | NSEventType::RightMouseDragged
        | NSEventType::OtherMouseDragged => {
            // Pen tablet motion is emitted as `TabletPoint` with richer pressure/tilt data.
            if pen_pointer_info.is_some() {
                return None;
            }
            let buttons_mask = NSEvent::pressedMouseButtons() as u64;
            PointerEvent::Move(PointerUpdate {
                pointer: map::pointer_info_primary_for_type(PointerType::Mouse),
                current: state(buttons_mask, None, None),
                coalesced: alloc::vec![],
                predicted: alloc::vec![],
            })
        }
        NSEventType::MouseEntered => {
            PointerEvent::Enter(map::pointer_info_primary_for_type(PointerType::Mouse))
        }
        NSEventType::MouseExited => {
            PointerEvent::Leave(map::pointer_info_primary_for_type(PointerType::Mouse))
        }
        NSEventType::ScrollWheel => {
            let precise = e.hasPreciseScrollingDeltas();
            // `scrollingDeltaX/Y` already include AppKit's natural-scrolling
            // preference adjustment. Preserve the sign to match winit's macOS
            // backend and leave content movement policy to downstream users.
            let dx = e.scrollingDeltaX();
            let dy = e.scrollingDeltaY();
            let buttons_mask = NSEvent::pressedMouseButtons() as u64;
            let state = state(buttons_mask, None, None);
            PointerEvent::Scroll(PointerScrollEvent {
                pointer: map::pointer_info_primary_for_type(PointerType::Mouse),
                delta: map::pointer_scroll_delta_from_raw(state.scale_factor, precise, dx, dy),
                state,
            })
        }
        NSEventType::Magnify => {
            let buttons_mask = NSEvent::pressedMouseButtons() as u64;
            let state = state(buttons_mask, None, None);
            let magnification = e.magnification();
            if !magnification.is_finite() {
                return None;
            }
            #[expect(
                clippy::cast_possible_truncation,
                reason = "AppKit reports magnification as f64; ui-events uses f32"
            )]
            let factor = magnification as f32;
            PointerEvent::Gesture(PointerGestureEvent {
                pointer: map::pointer_info_primary_for_type(PointerType::Mouse),
                gesture: PointerGesture::Pinch(factor),
                state,
            })
        }
        NSEventType::Rotate => {
            // AppKit reports rotation in degrees, where positive values are counterclockwise.
            // ui-events uses clockwise radians.
            let buttons_mask = NSEvent::pressedMouseButtons() as u64;
            let state = state(buttons_mask, None, None);
            let degrees = e.rotation() as f64;
            if !degrees.is_finite() {
                return None;
            }
            #[expect(
                clippy::cast_possible_truncation,
                reason = "AppKit reports degrees as f64; ui-events stores radians as f32"
            )]
            let radians_clockwise = (-degrees).to_radians() as f32;
            PointerEvent::Gesture(PointerGestureEvent {
                pointer: map::pointer_info_primary_for_type(PointerType::Mouse),
                gesture: PointerGesture::Rotate(radians_clockwise),
                state,
            })
        }
        NSEventType::TabletPoint => {
            let pointing_device = e.pointingDeviceType();
            let pointer_type = pointer_type_from_pointing_device(pointing_device);
            let buttons_mask = pen_buttons_mask(pointing_device, e.buttonMask().0 as u64);
            let pressure = Some(e.pressure() as f64);
            let tangential = Some(e.tangentialPressure() as f64);
            let tilt = e.tilt();
            let mut current = state(buttons_mask, pressure, tangential);
            current.orientation = map::orientation_from_tilt_fraction(tilt.x, tilt.y);
            let pointer = map::pointer_info_from_platform_ids(
                pointer_type,
                Some(e.uniqueID()),
                Some(e.deviceID() as u64),
            );
            PointerEvent::Move(PointerUpdate {
                pointer,
                current,
                coalesced: alloc::vec![],
                predicted: alloc::vec![],
            })
        }
        NSEventType::TabletProximity => {
            let pointing_device = e.pointingDeviceType();
            let pointer_type = pointer_type_from_pointing_device(pointing_device);
            let pointer = map::pointer_info_from_platform_ids(
                pointer_type,
                Some(e.uniqueID()),
                Some(e.deviceID() as u64),
            );
            if e.isEnteringProximity() {
                PointerEvent::Enter(pointer)
            } else {
                PointerEvent::Leave(pointer)
            }
        }
        _ => return None,
    })
}

fn map_virtual_keycode_to_code_named_location(code: u16) -> (Code, Option<NamedKey>, Location) {
    use vk as K;
    match code {
        K::ANSI_A => (Code::KeyA, None, Location::Standard),
        K::ANSI_S => (Code::KeyS, None, Location::Standard),
        K::ANSI_D => (Code::KeyD, None, Location::Standard),
        K::ANSI_F => (Code::KeyF, None, Location::Standard),
        K::ANSI_H => (Code::KeyH, None, Location::Standard),
        K::ANSI_G => (Code::KeyG, None, Location::Standard),
        K::ANSI_Z => (Code::KeyZ, None, Location::Standard),
        K::ANSI_X => (Code::KeyX, None, Location::Standard),
        K::ANSI_C => (Code::KeyC, None, Location::Standard),
        K::ANSI_V => (Code::KeyV, None, Location::Standard),
        K::ANSI_B => (Code::KeyB, None, Location::Standard),
        K::ANSI_Q => (Code::KeyQ, None, Location::Standard),
        K::ANSI_W => (Code::KeyW, None, Location::Standard),
        K::ANSI_E => (Code::KeyE, None, Location::Standard),
        K::ANSI_R => (Code::KeyR, None, Location::Standard),
        K::ANSI_Y => (Code::KeyY, None, Location::Standard),
        K::ANSI_T => (Code::KeyT, None, Location::Standard),
        K::ANSI_1 => (Code::Digit1, None, Location::Standard),
        K::ANSI_2 => (Code::Digit2, None, Location::Standard),
        K::ANSI_3 => (Code::Digit3, None, Location::Standard),
        K::ANSI_4 => (Code::Digit4, None, Location::Standard),
        K::ANSI_6 => (Code::Digit6, None, Location::Standard),
        K::ANSI_5 => (Code::Digit5, None, Location::Standard),
        K::ANSI_EQUAL => (Code::Equal, None, Location::Standard),
        K::ANSI_9 => (Code::Digit9, None, Location::Standard),
        K::ANSI_7 => (Code::Digit7, None, Location::Standard),
        K::ANSI_MINUS => (Code::Minus, None, Location::Standard),
        K::ANSI_8 => (Code::Digit8, None, Location::Standard),
        K::ANSI_0 => (Code::Digit0, None, Location::Standard),
        K::ANSI_RIGHT_BRACKET => (Code::BracketRight, None, Location::Standard),
        K::ANSI_O => (Code::KeyO, None, Location::Standard),
        K::ANSI_U => (Code::KeyU, None, Location::Standard),
        K::ANSI_LEFT_BRACKET => (Code::BracketLeft, None, Location::Standard),
        K::ANSI_I => (Code::KeyI, None, Location::Standard),
        K::ANSI_P => (Code::KeyP, None, Location::Standard),
        K::RETURN => (Code::Enter, Some(NamedKey::Enter), Location::Standard),
        K::ANSI_L => (Code::KeyL, None, Location::Standard),
        K::ANSI_J => (Code::KeyJ, None, Location::Standard),
        K::ANSI_QUOTE => (Code::Quote, None, Location::Standard),
        K::ANSI_K => (Code::KeyK, None, Location::Standard),
        K::ANSI_SEMICOLON => (Code::Semicolon, None, Location::Standard),
        K::ANSI_BACKSLASH => (Code::Backslash, None, Location::Standard),
        K::ANSI_COMMA => (Code::Comma, None, Location::Standard),
        K::ANSI_SLASH => (Code::Slash, None, Location::Standard),
        K::ANSI_N => (Code::KeyN, None, Location::Standard),
        K::ANSI_M => (Code::KeyM, None, Location::Standard),
        K::ANSI_PERIOD => (Code::Period, None, Location::Standard),
        K::TAB => (Code::Tab, Some(NamedKey::Tab), Location::Standard),
        // Space is treated as a character rather than NamedKey.
        K::SPACE => (Code::Space, None, Location::Standard),
        K::ANSI_GRAVE => (Code::Backquote, None, Location::Standard),
        K::DELETE => (
            Code::Backspace,
            Some(NamedKey::Backspace),
            Location::Standard,
        ),
        K::ESCAPE => (Code::Escape, Some(NamedKey::Escape), Location::Standard),
        K::RIGHT_COMMAND => (Code::MetaRight, Some(NamedKey::Meta), Location::Right),
        K::COMMAND => (Code::MetaLeft, Some(NamedKey::Meta), Location::Left),
        K::SHIFT => (Code::ShiftLeft, Some(NamedKey::Shift), Location::Left),
        K::CAPS_LOCK => (Code::CapsLock, Some(NamedKey::CapsLock), Location::Standard),
        K::OPTION => (Code::AltLeft, Some(NamedKey::Alt), Location::Left),
        K::CONTROL => (Code::ControlLeft, Some(NamedKey::Control), Location::Left),
        K::RIGHT_SHIFT => (Code::ShiftRight, Some(NamedKey::Shift), Location::Right),
        K::RIGHT_OPTION => (Code::AltRight, Some(NamedKey::Alt), Location::Right),
        K::RIGHT_CONTROL => (Code::ControlRight, Some(NamedKey::Control), Location::Right),
        K::FUNCTION => (Code::Fn, Some(NamedKey::Fn), Location::Standard),
        K::ANSI_KEYPAD_DECIMAL => (Code::NumpadDecimal, None, Location::Numpad),
        K::ANSI_KEYPAD_MULTIPLY => (Code::NumpadMultiply, None, Location::Numpad),
        K::ANSI_KEYPAD_PLUS => (Code::NumpadAdd, None, Location::Numpad),
        K::ANSI_KEYPAD_CLEAR => (Code::NumLock, Some(NamedKey::NumLock), Location::Numpad),
        K::ANSI_KEYPAD_DIVIDE => (Code::NumpadDivide, None, Location::Numpad),
        K::ANSI_KEYPAD_ENTER => (Code::NumpadEnter, Some(NamedKey::Enter), Location::Numpad),
        K::ANSI_KEYPAD_MINUS => (Code::NumpadSubtract, None, Location::Numpad),
        K::ANSI_KEYPAD_EQUALS => (Code::NumpadEqual, None, Location::Numpad),
        K::ANSI_KEYPAD_0 => (Code::Numpad0, None, Location::Numpad),
        K::ANSI_KEYPAD_1 => (Code::Numpad1, None, Location::Numpad),
        K::ANSI_KEYPAD_2 => (Code::Numpad2, None, Location::Numpad),
        K::ANSI_KEYPAD_3 => (Code::Numpad3, None, Location::Numpad),
        K::ANSI_KEYPAD_4 => (Code::Numpad4, None, Location::Numpad),
        K::ANSI_KEYPAD_5 => (Code::Numpad5, None, Location::Numpad),
        K::ANSI_KEYPAD_6 => (Code::Numpad6, None, Location::Numpad),
        K::ANSI_KEYPAD_7 => (Code::Numpad7, None, Location::Numpad),
        K::ANSI_KEYPAD_8 => (Code::Numpad8, None, Location::Numpad),
        K::ANSI_KEYPAD_9 => (Code::Numpad9, None, Location::Numpad),
        K::F1 => (Code::F1, Some(NamedKey::F1), Location::Standard),
        K::F2 => (Code::F2, Some(NamedKey::F2), Location::Standard),
        K::F3 => (Code::F3, Some(NamedKey::F3), Location::Standard),
        K::F4 => (Code::F4, Some(NamedKey::F4), Location::Standard),
        K::F5 => (Code::F5, Some(NamedKey::F5), Location::Standard),
        K::F6 => (Code::F6, Some(NamedKey::F6), Location::Standard),
        K::F7 => (Code::F7, Some(NamedKey::F7), Location::Standard),
        K::F8 => (Code::F8, Some(NamedKey::F8), Location::Standard),
        K::F9 => (Code::F9, Some(NamedKey::F9), Location::Standard),
        K::F10 => (Code::F10, Some(NamedKey::F10), Location::Standard),
        K::F11 => (Code::F11, Some(NamedKey::F11), Location::Standard),
        K::F12 => (Code::F12, Some(NamedKey::F12), Location::Standard),
        K::F13 => (Code::F13, Some(NamedKey::F13), Location::Standard),
        K::F14 => (Code::F14, Some(NamedKey::F14), Location::Standard),
        K::F15 => (Code::F15, Some(NamedKey::F15), Location::Standard),
        K::F16 => (Code::F16, Some(NamedKey::F16), Location::Standard),
        K::F17 => (Code::F17, Some(NamedKey::F17), Location::Standard),
        K::HELP => (Code::Help, Some(NamedKey::Help), Location::Standard),
        K::HOME => (Code::Home, Some(NamedKey::Home), Location::Standard),
        K::PAGE_UP => (Code::PageUp, Some(NamedKey::PageUp), Location::Standard),
        K::FORWARD_DELETE => (Code::Delete, Some(NamedKey::Delete), Location::Standard),
        K::END => (Code::End, Some(NamedKey::End), Location::Standard),
        K::PAGE_DOWN => (Code::PageDown, Some(NamedKey::PageDown), Location::Standard),
        K::ARROW_LEFT => (
            Code::ArrowLeft,
            Some(NamedKey::ArrowLeft),
            Location::Standard,
        ),
        K::ARROW_RIGHT => (
            Code::ArrowRight,
            Some(NamedKey::ArrowRight),
            Location::Standard,
        ),
        K::ARROW_DOWN => (
            Code::ArrowDown,
            Some(NamedKey::ArrowDown),
            Location::Standard,
        ),
        K::ARROW_UP => (Code::ArrowUp, Some(NamedKey::ArrowUp), Location::Standard),
        _ => (
            Code::Unidentified,
            Some(NamedKey::Unidentified),
            Location::Standard,
        ),
    }
}

fn map_modifier_toggle(code: u16) -> Option<(Code, NamedKey, Location, NSEventModifierFlags)> {
    use vk as K;
    Some(match code {
        K::SHIFT => (
            Code::ShiftLeft,
            NamedKey::Shift,
            Location::Left,
            NSEventModifierFlags::Shift,
        ),
        K::RIGHT_SHIFT => (
            Code::ShiftRight,
            NamedKey::Shift,
            Location::Right,
            NSEventModifierFlags::Shift,
        ),
        K::CONTROL => (
            Code::ControlLeft,
            NamedKey::Control,
            Location::Left,
            NSEventModifierFlags::Control,
        ),
        K::RIGHT_CONTROL => (
            Code::ControlRight,
            NamedKey::Control,
            Location::Right,
            NSEventModifierFlags::Control,
        ),
        K::OPTION => (
            Code::AltLeft,
            NamedKey::Alt,
            Location::Left,
            NSEventModifierFlags::Option,
        ),
        K::RIGHT_OPTION => (
            Code::AltRight,
            NamedKey::Alt,
            Location::Right,
            NSEventModifierFlags::Option,
        ),
        K::COMMAND => (
            Code::MetaLeft,
            NamedKey::Meta,
            Location::Left,
            NSEventModifierFlags::Command,
        ),
        K::RIGHT_COMMAND => (
            Code::MetaRight,
            NamedKey::Meta,
            Location::Right,
            NSEventModifierFlags::Command,
        ),
        K::CAPS_LOCK => (
            Code::CapsLock,
            NamedKey::CapsLock,
            Location::Standard,
            NSEventModifierFlags::CapsLock,
        ),
        _ => return None,
    })
}

fn key_from_appkit_strings(
    named: Option<NamedKey>,
    characters: Option<&str>,
    unmodified_characters: Option<&str>,
) -> Key {
    if let Some(named) = named {
        Key::Named(named)
    } else if let Some(chars) = characters.and_then(single_character_string) {
        Key::Character(chars.into())
    } else if let Some(chars) = unmodified_characters.and_then(single_character_string) {
        Key::Character(chars.into())
    } else {
        Key::Named(NamedKey::Unidentified)
    }
}

fn single_character_string(s: &str) -> Option<&str> {
    (s.chars().count() == 1).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;
    use ui_events::pointer::{PointerButton, PointerId, PointerType};

    #[test]
    fn tablet_pen_mouse_events_keep_pen_identity() {
        let pointer = pen_pointer_info_for_mouse_event(
            NSEventSubtype::TabletPoint,
            NSPointingDeviceType::Pen,
            41,
            7,
        )
        .expect("tablet-pen mouse events should preserve pen identity");
        assert_eq!(pointer.pointer_type, PointerType::Pen);
        assert_ne!(pointer.pointer_id, Some(PointerId::PRIMARY));
        assert!(pointer.persistent_device_id.is_some());
    }

    #[test]
    fn ordinary_mouse_events_do_not_claim_pen_identity() {
        assert_eq!(
            pen_pointer_info_for_mouse_event(
                NSEventSubtype::MouseEvent,
                NSPointingDeviceType::Pen,
                41,
                7,
            ),
            None
        );
    }

    #[test]
    fn eraser_transition_uses_pen_eraser_button() {
        assert_eq!(
            pen_transition_button(NSPointingDeviceType::Eraser, 0),
            Some(PointerButton::PenEraser)
        );
    }

    #[test]
    fn mouse_down_state_includes_transition_button() {
        assert_eq!(mouse_buttons_after_down(0, 0), 1);
        assert_eq!(mouse_buttons_after_down(1 << 1, 0), 0b11);
    }

    #[test]
    fn mouse_up_state_excludes_transition_button() {
        assert_eq!(mouse_buttons_after_up(1, 0), 0);
        assert_eq!(mouse_buttons_after_up(0b11, 0), 0b10);
    }

    #[test]
    fn mouse_transition_state_ignores_invalid_button_numbers() {
        assert_eq!(mouse_buttons_after_down(0b1010, -1), 0b1010);
        assert_eq!(mouse_buttons_after_up(0b1010, 32), 0b1010);
    }

    #[test]
    fn click_count_is_read_for_mouse_button_events() {
        let called = Cell::new(false);
        let count = click_count_for_mouse_button_event(NSEventType::LeftMouseDown, || {
            called.set(true);
            2
        });

        assert_eq!(count, 2);
        assert!(called.get());
    }

    #[test]
    fn scroll_click_count_is_zero_without_reading_event() {
        let called = Cell::new(false);
        let count = click_count_for_mouse_button_event(NSEventType::ScrollWheel, || {
            called.set(true);
            2
        });

        assert_eq!(count, 0);
        assert!(!called.get());
    }

    #[test]
    fn pen_pointer_info_is_read_for_mouse_events() {
        let called = Cell::new(false);
        let pointer = pen_pointer_info_for_mouse_nsevent(NSEventType::LeftMouseDragged, || {
            called.set(true);
            Some(map::pointer_info_from_platform_ids(
                PointerType::Pen,
                Some(1),
                Some(2),
            ))
        });

        assert!(pointer.is_some());
        assert!(called.get());
    }

    #[test]
    fn scroll_pen_pointer_info_is_not_read() {
        let called = Cell::new(false);
        let pointer = pen_pointer_info_for_mouse_nsevent(NSEventType::ScrollWheel, || {
            called.set(true);
            Some(map::pointer_info_from_platform_ids(
                PointerType::Pen,
                Some(1),
                Some(2),
            ))
        });

        assert_eq!(pointer, None);
        assert!(!called.get());
    }

    #[test]
    fn left_shift_modifier_toggle_preserves_code_and_location() {
        assert_eq!(
            map_modifier_toggle(vk::SHIFT),
            Some((
                Code::ShiftLeft,
                NamedKey::Shift,
                Location::Left,
                NSEventModifierFlags::Shift,
            ))
        );
    }

    #[test]
    fn right_command_modifier_toggle_preserves_code_and_location() {
        assert_eq!(
            map_modifier_toggle(vk::RIGHT_COMMAND),
            Some((
                Code::MetaRight,
                NamedKey::Meta,
                Location::Right,
                NSEventModifierFlags::Command,
            ))
        );
    }

    #[test]
    fn produced_characters_take_priority_over_unmodified_fallback() {
        assert_eq!(
            key_from_appkit_strings(None, Some("ß"), Some("s")),
            Key::Character("ß".into())
        );
    }

    #[test]
    fn single_unmodified_character_is_used_as_fallback() {
        assert_eq!(
            key_from_appkit_strings(None, None, Some("a")),
            Key::Character("a".into())
        );
    }

    #[test]
    fn multi_scalar_characters_fall_back_to_unmodified_character() {
        assert_eq!(
            key_from_appkit_strings(None, Some("de"), Some("d")),
            Key::Character("d".into())
        );
    }

    #[test]
    fn keypad_enter_maps_to_numpad_location() {
        assert_eq!(
            map_virtual_keycode_to_code_named_location(vk::ANSI_KEYPAD_ENTER),
            (Code::NumpadEnter, Some(NamedKey::Enter), Location::Numpad)
        );
    }

    #[test]
    fn f13_preserves_named_key() {
        assert_eq!(
            map_virtual_keycode_to_code_named_location(vk::F13),
            (Code::F13, Some(NamedKey::F13), Location::Standard)
        );
    }
}
