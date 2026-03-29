// Copyright 2025 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Support routines for converting pointer data from [`web_sys`].

use alloc::vec;
use alloc::vec::Vec;

use dpi::{PhysicalPosition, PhysicalSize};
use js_sys::{Array, Function, Reflect};
use ui_events::ScrollDelta;
use ui_events::keyboard::Modifiers;
use ui_events::pointer::{
    PointerButton, PointerButtonEvent, PointerButtons, PointerEvent, PointerId, PointerInfo,
    PointerOrientation, PointerState, PointerType, PointerUpdate,
};
use web_sys::wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    Element, Event, MouseEvent, PointerEvent as WebPointerEvent, Touch, TouchEvent, TouchList,
    WheelEvent,
};

#[inline]
#[expect(
    clippy::cast_possible_truncation,
    reason = "DOM timestamp is f64 ms; convert to integer ns intentionally"
)]
fn ms_to_ns_u64(ms: f64) -> u64 {
    (ms * 1_000_000.0) as u64
}

#[inline]
#[expect(
    clippy::cast_possible_truncation,
    reason = "DOM wheel line/page deltas are f64; ui-events stores f32"
)]
fn f64_to_f32_delta(v: f64) -> f32 {
    v as f32
}

/// Try to make a [`PointerButton`] from a [`web_sys::MouseEvent::button`].
///
/// Values less than 0 or greater than 31 will not be mapped.
///
/// This corresponds to §5.1.1.2 of the Pointer Events Level 2
/// specification.
pub fn try_from_web_button(b: i16) -> Option<PointerButton> {
    Some(match b {
        0 => PointerButton::Primary,
        // https://www.w3.org/TR/uievents/#dom-mouseevent-button
        // 1 = auxiliary (middle), 2 = secondary (right)
        1 => PointerButton::Auxiliary,
        2 => PointerButton::Secondary,
        3 => PointerButton::X1,
        4 => PointerButton::X2,
        5 => PointerButton::PenEraser,
        6 => PointerButton::B7,
        7 => PointerButton::B8,
        8 => PointerButton::B9,
        9 => PointerButton::B10,
        10 => PointerButton::B11,
        11 => PointerButton::B12,
        12 => PointerButton::B13,
        13 => PointerButton::B14,
        14 => PointerButton::B15,
        15 => PointerButton::B16,
        16 => PointerButton::B17,
        17 => PointerButton::B18,
        18 => PointerButton::B19,
        19 => PointerButton::B20,
        20 => PointerButton::B21,
        21 => PointerButton::B22,
        22 => PointerButton::B23,
        23 => PointerButton::B24,
        24 => PointerButton::B25,
        25 => PointerButton::B26,
        26 => PointerButton::B27,
        27 => PointerButton::B28,
        28 => PointerButton::B29,
        29 => PointerButton::B30,
        30 => PointerButton::B31,
        31 => PointerButton::B32,
        _ => {
            return None;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ui_events::pointer::PointerButton;

    #[test]
    fn web_mouse_button_mapping_matches_dom_spec() {
        assert_eq!(try_from_web_button(0), Some(PointerButton::Primary));
        assert_eq!(try_from_web_button(1), Some(PointerButton::Auxiliary));
        assert_eq!(try_from_web_button(2), Some(PointerButton::Secondary));
        assert_eq!(try_from_web_button(3), Some(PointerButton::X1));
        assert_eq!(try_from_web_button(4), Some(PointerButton::X2));
        assert_eq!(try_from_web_button(-1), None);
        assert_eq!(try_from_web_button(32), None);
    }
}

/// Convert a DOM `MouseEvent.buttons()` bitfield into [`PointerButtons`].
pub fn from_web_buttons_mask(mask: u16) -> PointerButtons {
    // Compute in u32 to avoid shifting a 16-bit value by >= 16 (which panics in debug on wasm).
    let mask32 = mask as u32;
    let mut out = PointerButtons::default();
    for (i, btn) in NONZERO_VARIANTS.iter().enumerate() {
        if (mask32 & (1_u32 << i)) != 0 {
            out.insert(*btn);
        }
    }
    out
}

const NONZERO_VARIANTS: [PointerButton; 32] = [
    PointerButton::Primary,
    PointerButton::Secondary,
    PointerButton::Auxiliary,
    PointerButton::X1,
    PointerButton::X2,
    PointerButton::PenEraser,
    PointerButton::B7,
    PointerButton::B8,
    PointerButton::B9,
    PointerButton::B10,
    PointerButton::B11,
    PointerButton::B12,
    PointerButton::B13,
    PointerButton::B14,
    PointerButton::B15,
    PointerButton::B16,
    PointerButton::B17,
    PointerButton::B18,
    PointerButton::B19,
    PointerButton::B20,
    PointerButton::B21,
    PointerButton::B22,
    PointerButton::B23,
    PointerButton::B24,
    PointerButton::B25,
    PointerButton::B26,
    PointerButton::B27,
    PointerButton::B28,
    PointerButton::B29,
    PointerButton::B30,
    PointerButton::B31,
    PointerButton::B32,
];

/// Build a basic [`PointerState`] from a [`MouseEvent`].
///
/// Prefer [`state_from_pointer_event`] when handling W3C Pointer Events,
/// as it includes richer data (pressure, width/height, etc.).
///
/// - Coordinates use `clientX/Y` scaled by `scale_factor` to approximate physical pixels.
/// - Pressure is 0.5 when any button is down, else 0.0.
pub fn state_from_mouse_event(e: &MouseEvent, scale_factor: f64) -> PointerState {
    let css_x = e.client_x() as f64;
    let css_y = e.client_y() as f64;
    let buttons = from_web_buttons_mask(e.buttons());
    let pressure = if buttons.is_empty() { 0.0 } else { 0.5 };
    let time_ns = ms_to_ns_u64(e.time_stamp());
    PointerState {
        time: time_ns, // ms -> ns
        position: PhysicalPosition {
            x: css_x * scale_factor,
            y: css_y * scale_factor,
        },
        buttons,
        modifiers: modifiers_from_mouse(e),
        count: e.detail().clamp(0, 255) as u8,
        contact_geometry: PhysicalSize {
            width: 1.0,
            height: 1.0,
        },
        orientation: Default::default(),
        pressure,
        tangential_pressure: 0.0,
        scale_factor,
    }
}

fn modifiers_from_mouse(e: &MouseEvent) -> Modifiers {
    let mut m = Modifiers::default();
    if e.ctrl_key() {
        m.insert(Modifiers::CONTROL);
    }
    if e.alt_key() {
        m.insert(Modifiers::ALT);
    }
    if e.shift_key() {
        m.insert(Modifiers::SHIFT);
    }
    if e.meta_key() {
        m.insert(Modifiers::META);
    }
    m
}

fn pointer_info_mouse() -> PointerInfo {
    PointerInfo {
        pointer_id: Some(PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type: PointerType::Mouse,
    }
}

/// Build a `Down` from a DOM `mousedown`/`pointerdown` represented as [`MouseEvent`].
///
/// Prefer [`down_from_pointer_event`] when handling W3C Pointer Events.
pub fn down_from_mouse_event(e: &MouseEvent, scale_factor: f64) -> PointerEvent {
    PointerEvent::Down(PointerButtonEvent {
        button: try_from_web_button(e.button()),
        pointer: pointer_info_mouse(),
        state: state_from_mouse_event(e, scale_factor),
    })
}

/// Build an `Up` from a DOM `mouseup`/`pointerup` represented as [`MouseEvent`].
///
/// Prefer [`up_from_pointer_event`] when handling W3C Pointer Events.
pub fn up_from_mouse_event(e: &MouseEvent, scale_factor: f64) -> PointerEvent {
    PointerEvent::Up(PointerButtonEvent {
        button: try_from_web_button(e.button()),
        pointer: pointer_info_mouse(),
        state: state_from_mouse_event(e, scale_factor),
    })
}

/// Build a `Move` from a DOM `mousemove`/`pointermove` represented as [`MouseEvent`].
///
/// Prefer [`move_from_pointer_event`] when handling W3C Pointer Events.
pub fn move_from_mouse_event(e: &MouseEvent, scale_factor: f64) -> PointerEvent {
    PointerEvent::Move(PointerUpdate {
        pointer: pointer_info_mouse(),
        current: state_from_mouse_event(e, scale_factor),
        coalesced: Vec::new(),
        predicted: Vec::new(),
    })
}

/// Build an `Enter` from a DOM `mouseenter`/`pointerenter`.
///
/// Prefer [`enter_from_pointer_event`] when handling W3C Pointer Events.
pub fn enter_from_mouse_event(_e: &MouseEvent) -> PointerEvent {
    PointerEvent::Enter(pointer_info_mouse())
}

/// Build a `Leave` from a DOM `mouseleave`/`pointerleave`.
///
/// Prefer [`leave_from_pointer_event`] when handling W3C Pointer Events.
pub fn leave_from_mouse_event(_e: &MouseEvent) -> PointerEvent {
    PointerEvent::Leave(pointer_info_mouse())
}

/// Build a `Scroll` from a DOM `wheel` event.
///
/// `scale_factor` controls conversion of CSS pixel deltas to physical pixels.
pub fn scroll_from_wheel_event(e: &WheelEvent, scale_factor: f64) -> PointerEvent {
    let delta = match e.delta_mode() {
        WheelEvent::DOM_DELTA_PIXEL => ScrollDelta::PixelDelta(PhysicalPosition {
            x: e.delta_x() * scale_factor,
            y: e.delta_y() * scale_factor,
        }),
        WheelEvent::DOM_DELTA_LINE => {
            ScrollDelta::LineDelta(f64_to_f32_delta(e.delta_x()), f64_to_f32_delta(e.delta_y()))
        }
        WheelEvent::DOM_DELTA_PAGE => {
            ScrollDelta::PageDelta(f64_to_f32_delta(e.delta_x()), f64_to_f32_delta(e.delta_y()))
        }
        _ => ScrollDelta::PixelDelta(PhysicalPosition { x: 0.0, y: 0.0 }),
    };

    let me: &MouseEvent = e;
    PointerEvent::Scroll(ui_events::pointer::PointerScrollEvent {
        pointer: pointer_info_mouse(),
        delta,
        state: state_from_mouse_event(me, scale_factor),
    })
}

// PointerEvent (Web) conversions

fn pointer_type_from_str(s: &str) -> PointerType {
    match s {
        "mouse" => PointerType::Mouse,
        "pen" => PointerType::Pen,
        "touch" => PointerType::Touch,
        _ => PointerType::Unknown,
    }
}

fn pointer_info_from_web_pointer(e: &WebPointerEvent) -> PointerInfo {
    let id = if e.is_primary() {
        Some(PointerId::PRIMARY)
    } else {
        let raw = e.pointer_id() as u64;
        // Shift non-primary ids by +1 to avoid colliding with PRIMARY (1).
        PointerId::new(raw.saturating_add(1))
    };
    PointerInfo {
        pointer_id: id,
        persistent_device_id: None,
        pointer_type: pointer_type_from_str(&e.pointer_type()),
    }
}

fn modifiers_from_pointer(e: &WebPointerEvent) -> Modifiers {
    let mut m = Modifiers::default();
    if e.ctrl_key() {
        m.insert(Modifiers::CONTROL);
    }
    if e.alt_key() {
        m.insert(Modifiers::ALT);
    }
    if e.shift_key() {
        m.insert(Modifiers::SHIFT);
    }
    if e.meta_key() {
        m.insert(Modifiers::META);
    }
    m
}

fn orientation_from_pointer_event(e: &WebPointerEvent) -> PointerOrientation {
    // Prefer Pointer Events Level 3 altitude/azimuth when present (radians).
    let obj = e.as_ref();
    if let (Ok(alt), Ok(azi)) = (
        Reflect::get(obj, &JsValue::from_str("altitudeAngle")),
        Reflect::get(obj, &JsValue::from_str("azimuthAngle")),
    ) {
        if let (Some(alt), Some(azi)) = (alt.as_f64(), azi.as_f64()) {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "DOM provides f64 radians; ui-events stores orientation as f32"
            )]
            return PointerOrientation {
                altitude: alt as f32,
                azimuth: azi as f32,
            };
        }
    }

    // Fall back to Pointer Events tiltX/tiltY (degrees).
    // tiltX/tiltY are in [-90, 90]. Avoid tan() singularities at 90 degrees.
    let tilt_x = (e.tilt_x() as f32).clamp(-89.9, 89.9);
    let tilt_y = (e.tilt_y() as f32).clamp(-89.9, 89.9);
    pointer_orientation_from_tilt_degrees(tilt_x, tilt_y)
}

fn pointer_orientation_from_tilt_degrees(tilt_x_deg: f32, tilt_y_deg: f32) -> PointerOrientation {
    let tx = tilt_x_deg.to_radians();
    let ty = tilt_y_deg.to_radians();
    let x = tx.tan();
    let y = ty.tan();

    // Model the pen axis as the normalized vector (x, y, 1), where x/z = tan(tiltX),
    // y/z = tan(tiltY). When perpendicular: (0,0,1).
    let inv_norm = 1.0 / (x.mul_add(x, y * y) + 1.0).sqrt();
    let z = inv_norm;

    let altitude = z.asin();
    let azimuth = if x == 0.0 && y == 0.0 {
        core::f32::consts::FRAC_PI_2
    } else {
        y.atan2(x)
    };

    PointerOrientation { altitude, azimuth }
}

/// Build a [`PointerState`] from a DOM [`web_sys::PointerEvent`].
///
/// - Coordinates use `clientX/Y` scaled by `scale_factor` to approximate
///   physical pixels.
/// - Uses the event's reported `pressure`, `tangentialPressure`, `width/height`, and
///   stylus orientation where available (preferring `altitudeAngle`/`azimuthAngle`,
///   otherwise falling back to `tiltX`/`tiltY`).
/// - Pointer Events `twist` is not currently mapped (there is no corresponding field in
///   `ui-events`).
pub fn state_from_pointer_event(e: &WebPointerEvent, scale_factor: f64) -> PointerState {
    let css_x = e.client_x() as f64;
    let css_y = e.client_y() as f64;
    let buttons = from_web_buttons_mask(e.buttons());
    let pressure = e.pressure();
    let tangential_pressure = e.tangential_pressure();
    let width = e.width() as f64 * scale_factor;
    let height = e.height() as f64 * scale_factor;
    let time_ns = ms_to_ns_u64(e.time_stamp());
    PointerState {
        time: time_ns,
        position: PhysicalPosition {
            x: css_x * scale_factor,
            y: css_y * scale_factor,
        },
        buttons,
        modifiers: modifiers_from_pointer(e),
        count: e.detail().clamp(0, 255) as u8,
        contact_geometry: PhysicalSize { width, height },
        orientation: orientation_from_pointer_event(e),
        pressure,
        tangential_pressure,
        scale_factor,
    }
}

/// Build a [`PointerEvent::Down`] from a DOM `pointerdown`.
pub fn down_from_pointer_event(e: &WebPointerEvent, scale_factor: f64) -> PointerEvent {
    PointerEvent::Down(PointerButtonEvent {
        button: try_from_web_button(e.button()),
        pointer: pointer_info_from_web_pointer(e),
        state: state_from_pointer_event(e, scale_factor),
    })
}

/// Build an [`PointerEvent::Up`] from a DOM `pointerup`.
pub fn up_from_pointer_event(e: &WebPointerEvent, scale_factor: f64) -> PointerEvent {
    PointerEvent::Up(PointerButtonEvent {
        button: try_from_web_button(e.button()),
        pointer: pointer_info_from_web_pointer(e),
        state: state_from_pointer_event(e, scale_factor),
    })
}

/// Controls how pointer events are converted.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Scale factor to convert CSS pixels to physical pixels.
    pub scale_factor: f64,
    /// Whether to collect coalesced move samples.
    pub collect_coalesced: bool,
    /// Whether to collect predicted move samples.
    pub collect_predicted: bool,
}

impl Default for Options {
    fn default() -> Self {
        // Defaults avoid allocations on hot paths; enable explicitly when desired.
        Self {
            scale_factor: 1.0,
            collect_coalesced: false,
            collect_predicted: false,
        }
    }
}

impl Options {
    /// Set the scale factor (builder style).
    pub fn with_scale(mut self, scale: f64) -> Self {
        self.scale_factor = scale;
        self
    }
    /// Set whether to collect coalesced samples.
    pub fn with_coalesced(mut self, enabled: bool) -> Self {
        self.collect_coalesced = enabled;
        self
    }
    /// Set whether to collect predicted samples.
    pub fn with_predicted(mut self, enabled: bool) -> Self {
        self.collect_predicted = enabled;
        self
    }
}

/// Build a `Move` from a DOM `pointermove`, with conversion options.
pub fn move_from_pointer_event(e: &WebPointerEvent, opts: &Options) -> PointerEvent {
    let pointer = pointer_info_from_web_pointer(e);
    let current = state_from_pointer_event(e, opts.scale_factor);

    let coalesced_states = if opts.collect_coalesced {
        get_coalesced_events_safe(e, opts.scale_factor)
    } else {
        Vec::new()
    };

    let predicted_states = if opts.collect_predicted {
        get_predicted_events_safe(e, opts.scale_factor)
    } else {
        Vec::new()
    };

    PointerEvent::Move(PointerUpdate {
        pointer,
        current,
        coalesced: coalesced_states,
        predicted: predicted_states,
    })
}

fn collect_states_from_array(arr: &Array, scale_factor: f64) -> Vec<PointerState> {
    let mut out = Vec::new();
    let len = arr.length();
    for i in 0..len {
        let v = arr.get(i);
        if let Ok(pe) = v.dyn_into::<WebPointerEvent>() {
            out.push(state_from_pointer_event(&pe, scale_factor));
        }
    }
    out
}

fn get_coalesced_events_safe(e: &WebPointerEvent, scale_factor: f64) -> Vec<PointerState> {
    let obj = e.as_ref();
    let Ok(v) = Reflect::get(
        obj,
        &web_sys::wasm_bindgen::JsValue::from_str("getCoalescedEvents"),
    ) else {
        return Vec::new();
    };
    if !v.is_function() {
        return Vec::new();
    }
    let f: Function = v.unchecked_into();
    let Ok(jsarr) = f.call0(obj) else {
        return Vec::new();
    };
    let Ok(arr) = jsarr.dyn_into::<Array>() else {
        return Vec::new();
    };
    collect_states_from_array(&arr, scale_factor)
}

fn get_predicted_events_safe(e: &WebPointerEvent, scale_factor: f64) -> Vec<PointerState> {
    let obj = e.as_ref();
    let Ok(v) = Reflect::get(
        obj,
        &web_sys::wasm_bindgen::JsValue::from_str("getPredictedEvents"),
    ) else {
        return Vec::new();
    };
    if !v.is_function() {
        return Vec::new();
    }
    let f: Function = v.unchecked_into();
    let Ok(jsarr) = f.call0(obj) else {
        return Vec::new();
    };
    let Ok(arr) = jsarr.dyn_into::<Array>() else {
        return Vec::new();
    };
    collect_states_from_array(&arr, scale_factor)
}

/// Build an [`PointerEvent::Enter`] from a DOM `pointerenter`.
pub fn enter_from_pointer_event(e: &WebPointerEvent) -> PointerEvent {
    PointerEvent::Enter(pointer_info_from_web_pointer(e))
}

/// Build a [`PointerEvent::Leave`] from a DOM `pointerleave`.
pub fn leave_from_pointer_event(e: &WebPointerEvent) -> PointerEvent {
    PointerEvent::Leave(pointer_info_from_web_pointer(e))
}

/// Build a [`PointerEvent::Cancel`] from a DOM `pointercancel`.
pub fn cancel_from_pointer_event(e: &WebPointerEvent) -> PointerEvent {
    PointerEvent::Cancel(pointer_info_from_web_pointer(e))
}

/// Convert a DOM `TouchEvent` into zero or more `ui-events` [`PointerEvent`]s.
///
/// Browser touch events can report multiple changed touches at once, so this returns a `Vec`.
/// For `touchstart`, `touchmove`, and `touchend`, the returned events correspond to the
/// event's `changedTouches` list.
///
/// For `touchcancel`, the returned events are [`PointerEvent::Cancel`], which do not include
/// pointer state.
pub fn pointer_events_from_touch_event(ev: &TouchEvent, opts: &Options) -> Vec<PointerEvent> {
    let time_ns = ms_to_ns_u64(ev.time_stamp());
    let modifiers = modifiers_from_touch(ev);

    let touch_count = pointer_attach_count_from_active_touches(ev.touches().length());
    let primary_identifier = min_touch_identifier_from_event(ev);

    let type_ = ev.type_();
    let changed = ev.changed_touches();

    let mut out = Vec::new();
    let len = changed.length();
    for i in 0..len {
        let Some(touch) = changed.item(i) else {
            continue;
        };
        let pointer = pointer_info_from_touch(&touch, primary_identifier);
        match type_.as_str() {
            "touchstart" => out.push(PointerEvent::Down(PointerButtonEvent {
                button: None,
                pointer,
                state: state_from_touch(&touch, time_ns, modifiers, touch_count, opts.scale_factor),
            })),
            "touchmove" => out.push(PointerEvent::Move(PointerUpdate {
                pointer,
                current: state_from_touch(
                    &touch,
                    time_ns,
                    modifiers,
                    touch_count,
                    opts.scale_factor,
                ),
                coalesced: Vec::new(),
                predicted: Vec::new(),
            })),
            "touchend" => out.push(PointerEvent::Up(PointerButtonEvent {
                button: None,
                pointer,
                state: state_from_touch_end(
                    &touch,
                    time_ns,
                    modifiers,
                    touch_count,
                    opts.scale_factor,
                ),
            })),
            "touchcancel" => out.push(PointerEvent::Cancel(pointer)),
            _ => {}
        }
    }
    out
}

fn modifiers_from_touch(e: &TouchEvent) -> Modifiers {
    let mut m = Modifiers::default();
    if e.ctrl_key() {
        m.insert(Modifiers::CONTROL);
    }
    if e.alt_key() {
        m.insert(Modifiers::ALT);
    }
    if e.shift_key() {
        m.insert(Modifiers::SHIFT);
    }
    if e.meta_key() {
        m.insert(Modifiers::META);
    }
    m
}

fn min_touch_identifier_from_event(ev: &TouchEvent) -> Option<u64> {
    let mut min = min_touch_identifier(&ev.touches())?;
    if let Some(changed_min) = min_touch_identifier(&ev.changed_touches()) {
        min = min.min(changed_min);
    }
    Some(min)
}

fn min_touch_identifier(list: &TouchList) -> Option<u64> {
    let mut min: Option<u64> = None;
    let len = list.length();
    for i in 0..len {
        let Some(t) = list.item(i) else {
            continue;
        };
        let id = touch_identifier_u64(&t)?;
        min = Some(min.map_or(id, |m| m.min(id)));
    }
    min
}

fn touch_identifier_u64(touch: &Touch) -> Option<u64> {
    let id = touch.identifier();
    if id < 0 {
        return None;
    }
    Some(id as u64)
}

fn pointer_id_from_touch_identifier(id: i32, primary_identifier: Option<u64>) -> Option<PointerId> {
    if id < 0 {
        return None;
    }
    let id_u64 = id as u64;
    if primary_identifier.is_some_and(|p| p == id_u64) {
        return Some(PointerId::PRIMARY);
    }
    PointerId::new(id_u64.saturating_add(2))
}

fn pointer_attach_count_from_active_touches(active_touches: u32) -> u8 {
    active_touches.min(255) as u8
}

fn pointer_info_from_touch(touch: &Touch, primary_identifier: Option<u64>) -> PointerInfo {
    PointerInfo {
        pointer_id: pointer_id_from_touch_identifier(touch.identifier(), primary_identifier),
        persistent_device_id: None,
        pointer_type: PointerType::Touch,
    }
}

fn state_from_touch(
    touch: &Touch,
    time_ns: u64,
    modifiers: Modifiers,
    touch_count: u8,
    scale_factor: f64,
) -> PointerState {
    let css_x = touch.client_x() as f64;
    let css_y = touch.client_y() as f64;

    // Touch.radiusX/Y are radii in CSS pixels; `PointerState` stores a size.
    let width_css = (touch.radius_x() as f64 * 2.0).max(1.0);
    let height_css = (touch.radius_y() as f64 * 2.0).max(1.0);

    let pressure = {
        let f = touch.force();
        if f > 0.0 { f } else { 0.5 }
    };

    PointerState {
        time: time_ns,
        position: PhysicalPosition {
            x: css_x * scale_factor,
            y: css_y * scale_factor,
        },
        buttons: PointerButtons::default(),
        modifiers,
        count: touch_count,
        contact_geometry: PhysicalSize {
            width: width_css * scale_factor,
            height: height_css * scale_factor,
        },
        orientation: Default::default(),
        pressure,
        tangential_pressure: 0.0,
        scale_factor,
    }
}

fn state_from_touch_end(
    touch: &Touch,
    time_ns: u64,
    modifiers: Modifiers,
    touch_count: u8,
    scale_factor: f64,
) -> PointerState {
    let mut s = state_from_touch(touch, time_ns, modifiers, touch_count, scale_factor);
    s.pressure = 0.0;
    s
}

/// Convert a DOM event (Touch/Mouse/Pointer/Wheel) into zero or more `ui-events`
/// [`PointerEvent`]s with options to control conversion.
pub fn pointer_events_from_dom_event(ev: &Event, opts: &Options) -> Vec<PointerEvent> {
    if let Some(te) = ev.dyn_ref::<TouchEvent>() {
        let out = pointer_events_from_touch_event(te, opts);
        if !out.is_empty() {
            return out;
        }
    }

    if let Some(wheel) = ev.dyn_ref::<WheelEvent>() {
        return vec![scroll_from_wheel_event(wheel, opts.scale_factor)];
    }
    if let Some(pe) = ev.dyn_ref::<WebPointerEvent>() {
        let Some(out) = (match pe.type_().as_str() {
            "pointerdown" => Some(down_from_pointer_event(pe, opts.scale_factor)),
            "pointerup" => Some(up_from_pointer_event(pe, opts.scale_factor)),
            "pointermove" => Some(move_from_pointer_event(pe, opts)),
            "pointerenter" => Some(enter_from_pointer_event(pe)),
            "pointerleave" => Some(leave_from_pointer_event(pe)),
            "pointercancel" => Some(cancel_from_pointer_event(pe)),
            _ => None,
        }) else {
            return Vec::new();
        };
        return vec![out];
    }
    if let Some(me) = ev.dyn_ref::<MouseEvent>() {
        let Some(out) = (match me.type_().as_str() {
            "mousedown" => Some(down_from_mouse_event(me, opts.scale_factor)),
            "mouseup" => Some(up_from_mouse_event(me, opts.scale_factor)),
            "mousemove" => Some(move_from_mouse_event(me, opts.scale_factor)),
            "mouseenter" => Some(enter_from_mouse_event(me)),
            "mouseleave" => Some(leave_from_mouse_event(me)),
            _ => None,
        }) else {
            return Vec::new();
        };
        return vec![out];
    }
    Vec::new()
}

/// Convert a DOM event (Mouse/Pointer/Wheel) into a `ui-events` [`PointerEvent`]
/// with options to control conversion.
///
/// For multi-touch events, this returns the primary pointer's event when possible,
/// otherwise it returns an arbitrary changed touch.
pub fn pointer_event_from_dom_event(ev: &Event, opts: &Options) -> Option<PointerEvent> {
    let mut events = pointer_events_from_dom_event(ev, opts);
    if events.is_empty() {
        return None;
    }
    if let Some(primary_idx) = events.iter().position(PointerEvent::is_primary_pointer) {
        return Some(events.swap_remove(primary_idx));
    }
    events.into_iter().next()
}

/// Set pointer capture on an element using the id from a `PointerEvent`.
pub fn set_pointer_capture(
    el: &Element,
    e: &WebPointerEvent,
) -> Result<(), web_sys::js_sys::JsString> {
    Ok(el.set_pointer_capture(e.pointer_id())?)
}

/// Release pointer capture on an element using the id from a `PointerEvent`.
pub fn release_pointer_capture(
    el: &Element,
    e: &WebPointerEvent,
) -> Result<(), web_sys::js_sys::JsString> {
    Ok(el.release_pointer_capture(e.pointer_id())?)
}

/// Query whether an element currently has capture for this pointer id.
pub fn has_pointer_capture(el: &Element, e: &WebPointerEvent) -> bool {
    el.has_pointer_capture(e.pointer_id())
}

#[cfg(test)]
mod touch_tests {
    use super::*;

    #[test]
    fn touch_identifier_to_pointer_id_mapping() {
        assert_eq!(
            pointer_id_from_touch_identifier(0, Some(0)),
            Some(PointerId::PRIMARY)
        );
        assert_eq!(
            pointer_id_from_touch_identifier(0, Some(1)),
            PointerId::new(2)
        );
        assert_eq!(
            pointer_id_from_touch_identifier(1, Some(1)),
            Some(PointerId::PRIMARY)
        );
        assert_eq!(
            pointer_id_from_touch_identifier(1, Some(0)),
            PointerId::new(3)
        );
        assert_eq!(pointer_id_from_touch_identifier(-1, Some(0)), None);
    }

    #[test]
    fn touch_count_clamps_to_u8() {
        assert_eq!(pointer_attach_count_from_active_touches(0), 0);
        assert_eq!(pointer_attach_count_from_active_touches(1), 1);
        assert_eq!(pointer_attach_count_from_active_touches(255), 255);
        assert_eq!(pointer_attach_count_from_active_touches(256), 255);
        assert_eq!(pointer_attach_count_from_active_touches(u32::MAX), 255);
    }
}

#[cfg(test)]
mod stylus_orientation_tests {
    use super::*;

    fn assert_approx(a: f32, b: f32, eps: f32) {
        assert!((a - b).abs() <= eps, "expected {a} ~= {b} (eps={eps})");
    }

    fn angle_wrap_pi(mut a: f32) -> f32 {
        // Wrap to (-pi, pi].
        const TWO_PI: f32 = core::f32::consts::PI * 2.0;
        a = (a + core::f32::consts::PI).rem_euclid(TWO_PI) - core::f32::consts::PI;
        if a <= -core::f32::consts::PI {
            a += TWO_PI;
        }
        a
    }

    fn assert_azimuth_approx(a: f32, b: f32, eps: f32) {
        let da = angle_wrap_pi(a - b).abs();
        assert!(
            da <= eps,
            "expected azimuth {a} ~= {b} (|Δ|={da}, eps={eps})"
        );
    }

    #[test]
    fn perpendicular_tilt_maps_to_perpendicular_altitude() {
        let o = pointer_orientation_from_tilt_degrees(0.0, 0.0);
        assert!((o.altitude - core::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn azimuth_matches_axes() {
        // Positive X => azimuth ~ 0
        let o = pointer_orientation_from_tilt_degrees(30.0, 0.0);
        assert_azimuth_approx(o.azimuth, 0.0, 1e-6);

        // Negative X => azimuth ~ pi
        let o = pointer_orientation_from_tilt_degrees(-30.0, 0.0);
        assert_azimuth_approx(o.azimuth, core::f32::consts::PI, 1e-6);

        // Positive Y => azimuth ~ pi/2
        let o = pointer_orientation_from_tilt_degrees(0.0, 30.0);
        assert_azimuth_approx(o.azimuth, core::f32::consts::FRAC_PI_2, 1e-6);

        // Negative Y => azimuth ~ -pi/2
        let o = pointer_orientation_from_tilt_degrees(0.0, -30.0);
        assert_azimuth_approx(o.azimuth, -core::f32::consts::FRAC_PI_2, 1e-6);
    }

    #[test]
    fn increasing_tilt_reduces_altitude() {
        let o0 = pointer_orientation_from_tilt_degrees(0.0, 0.0);
        let o1 = pointer_orientation_from_tilt_degrees(30.0, 0.0);
        let o2 = pointer_orientation_from_tilt_degrees(60.0, 0.0);
        assert!(o1.altitude < o0.altitude);
        assert!(o2.altitude < o1.altitude);
    }

    #[test]
    fn symmetry_negating_tilt_flips_azimuth_by_pi() {
        let o = pointer_orientation_from_tilt_degrees(25.0, -10.0);
        let o_neg = pointer_orientation_from_tilt_degrees(-25.0, 10.0);

        // `f32` trig can differ by just over 1e-6 radians on Miri's
        // cross-target interpreter while preserving the symmetry invariant.
        assert_approx(o.altitude, o_neg.altitude, 2e-6);
        assert_azimuth_approx(o_neg.azimuth, o.azimuth + core::f32::consts::PI, 1e-6);
    }

    #[test]
    fn near_ninety_degree_tilt_is_finite_and_near_parallel() {
        let o = pointer_orientation_from_tilt_degrees(89.9, 0.0);
        assert!(o.altitude.is_finite());
        assert!(o.azimuth.is_finite());
        assert!(o.altitude < 0.01);

        let o = pointer_orientation_from_tilt_degrees(-89.9, 0.0);
        assert!(o.altitude.is_finite());
        assert!(o.azimuth.is_finite());
        assert!(o.altitude < 0.01);

        let o = pointer_orientation_from_tilt_degrees(0.0, 89.9);
        assert!(o.altitude.is_finite());
        assert!(o.azimuth.is_finite());
        assert!(o.altitude < 0.01);
    }
}
