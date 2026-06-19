// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Platform-neutral conversions from Wayland input primitives into
//! [`ui-events`] building blocks.
//!
//! Every function here operates on plain values — evdev codes, `f64`
//! coordinates, axis values, and modifier booleans — and never references
//! `wayland-client` types or performs any foreign-function calls. That keeps
//! the module compilable and unit-testable on every target, including the
//! `no_std` cross-compilation and Miri jobs, and is what keeps [`ui-events`]
//! and `dpi` used on non-Linux targets.
//!
//! The Linux-gated reducers convert `wayland-client` event streams into the
//! arguments these helpers accept.
//!
//! [`ui-events`]: https://docs.rs/ui-events/

use dpi::PhysicalPosition;
use ui_events::ScrollDelta;
use ui_events::keyboard::{Code, Key, Location, Modifiers, NamedKey};
use ui_events::pointer::{
    ContactGeometry, PointerButton, PointerId, PointerInfo, PointerOrientation, PointerType,
};

// evdev pointer button codes from `linux/input-event-codes.h`.
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
const BTN_SIDE: u32 = 0x113;
const BTN_EXTRA: u32 = 0x114;
const BTN_FORWARD: u32 = 0x115;
const BTN_BACK: u32 = 0x116;
const BTN_TASK: u32 = 0x117;
// evdev tablet-tool button codes from `linux/input-event-codes.h`.
const BTN_STYLUS3: u32 = 0x149;
const BTN_STYLUS: u32 = 0x14b;
const BTN_STYLUS2: u32 = 0x14c;

/// Map an evdev pointer button code to a [`PointerButton`].
///
/// The codes are the `BTN_*` values from `linux/input-event-codes.h` that
/// `wl_pointer` reports in its button events:
///
/// - `BTN_LEFT` → [`PointerButton::Primary`]
/// - `BTN_RIGHT` → [`PointerButton::Secondary`]
/// - `BTN_MIDDLE` → [`PointerButton::Auxiliary`]
/// - `BTN_SIDE` → [`PointerButton::X1`]
/// - `BTN_EXTRA` → [`PointerButton::X2`]
/// - `BTN_FORWARD` → [`PointerButton::B7`]
/// - `BTN_BACK` → [`PointerButton::B8`]
/// - `BTN_TASK` → [`PointerButton::B9`]
///
/// `BTN_FORWARD`, `BTN_BACK`, and `BTN_TASK` have no dedicated `ui-events`
/// button, so they map into the generic `B7`..`B9` range rather than being
/// conflated with the [`PointerButton::X1`]/[`PointerButton::X2`] side buttons.
/// Any other code returns `None`.
pub fn pointer_button_from_evdev(code: u32) -> Option<PointerButton> {
    Some(match code {
        BTN_LEFT => PointerButton::Primary,
        BTN_RIGHT => PointerButton::Secondary,
        BTN_MIDDLE => PointerButton::Auxiliary,
        BTN_SIDE => PointerButton::X1,
        BTN_EXTRA => PointerButton::X2,
        BTN_FORWARD => PointerButton::B7,
        BTN_BACK => PointerButton::B8,
        BTN_TASK => PointerButton::B9,
        _ => return None,
    })
}

/// Map an evdev tablet-tool button code to a [`PointerButton`].
///
/// `zwp_tablet_tool_v2` reports stylus barrel buttons by their
/// `linux/input-event-codes.h` `BTN_STYLUS*` codes:
///
/// - `BTN_STYLUS` → [`PointerButton::Secondary`], which `ui-events` documents as
///   the pen barrel button.
/// - `BTN_STYLUS2` → [`PointerButton::B7`]
/// - `BTN_STYLUS3` → [`PointerButton::B8`]
///
/// The second and third barrel buttons have no dedicated `ui-events` button, so
/// they map into the generic `B7`/`B8` range rather than being conflated with
/// the mouse side or auxiliary buttons, mirroring how
/// [`pointer_button_from_evdev`] treats `BTN_FORWARD`/`BTN_BACK`/`BTN_TASK`. Any
/// other code defers to [`pointer_button_from_evdev`], so a tablet tool used as a
/// puck (the mouse and lens tool types) reuses the pointer-button table.
pub fn pen_button_from_evdev(code: u32) -> Option<PointerButton> {
    match code {
        BTN_STYLUS => Some(PointerButton::Secondary),
        BTN_STYLUS2 => Some(PointerButton::B7),
        BTN_STYLUS3 => Some(PointerButton::B8),
        other => pointer_button_from_evdev(other),
    }
}

/// Convert Wayland surface-local logical coordinates to physical pixels.
///
/// `wl_pointer` and `wl_touch` deliver surface-local coordinates as logical
/// (post-scale) `f64` values; the bindings already convert the on-the-wire
/// 24.8 fixed-point representation to `f64`, so no fixed-point conversion is
/// needed here. Multiplying by `scale_factor` yields the physical pixels that
/// [`PointerState`] positions use.
///
/// Non-finite coordinates are treated as `0.0`, and a non-positive or
/// non-finite `scale_factor` falls back to `1.0`.
///
/// [`PointerState`]: ui_events::pointer::PointerState
pub fn physical_position_from_logical(x: f64, y: f64, scale_factor: f64) -> PhysicalPosition<f64> {
    let scale_factor = positive_finite_or(scale_factor, 1.0);
    PhysicalPosition {
        x: finite_or(x, 0.0) * scale_factor,
        y: finite_or(y, 0.0) * scale_factor,
    }
}

/// The scroll axis source reported by `wl_pointer`.
///
/// This mirrors `wl_pointer::AxisSource` as plain values so this module needs
/// no `wayland-client` dependency; the reducer translates the protocol enum
/// into this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisSource {
    /// A physical scroll wheel with discrete detents.
    Wheel,
    /// A finger on a touchpad.
    Finger,
    /// Continuous, unbounded scrolling (for example a trackball or ring).
    Continuous,
    /// A tilting scroll wheel.
    WheelTilt,
}

/// One `wl_pointer` frame's accumulated scroll, expressed as plain values.
///
/// `wl_pointer` batches axis events between `frame` events. A reducer sums each
/// signal over a frame and then calls [`scroll_delta_from_axis_frame`] once per
/// frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AxisFrame {
    /// The axis source for this frame, if the compositor reported one.
    pub source: Option<AxisSource>,
    /// Continuous axis value in logical pixels, `(x, y)`.
    pub value: (f64, f64),
    /// High-resolution wheel steps where `120` equals one detent, `(x, y)`
    /// (`wl_pointer::axis_value120`, version 8+).
    pub value120: (i32, i32),
    /// Deprecated discrete detent count, `(x, y)` (`wl_pointer::axis_discrete`).
    pub discrete: (i32, i32),
}

/// Convert an accumulated [`AxisFrame`] into a [`ScrollDelta`].
///
/// Returns `None` when the frame carries no scroll motion.
///
/// Wheel sources (and frames with no reported source) produce a
/// [`ScrollDelta::LineDelta`] measured in wheel detents: high-resolution
/// `value120` is preferred (divided by `120`), falling back to the deprecated
/// discrete count, and finally to the continuous logical-pixel value as a
/// [`ScrollDelta::PixelDelta`] when no detent signal is present.
///
/// Finger and continuous sources produce a [`ScrollDelta::PixelDelta`] in
/// physical pixels (`value * scale_factor`).
///
/// Wayland's positive axis values mean down/right, matching [`ScrollDelta`]'s
/// Y-down convention, so signs are preserved.
pub fn scroll_delta_from_axis_frame(frame: AxisFrame, scale_factor: f64) -> Option<ScrollDelta> {
    let scale_factor = positive_finite_or(scale_factor, 1.0);
    let pixel_delta = |(x, y): (f64, f64)| {
        ScrollDelta::PixelDelta(PhysicalPosition {
            x: finite_or(x, 0.0) * scale_factor,
            y: finite_or(y, 0.0) * scale_factor,
        })
    };
    match frame.source {
        Some(AxisSource::Finger | AxisSource::Continuous) => {
            let (x, y) = frame.value;
            (x != 0.0 || y != 0.0).then(|| pixel_delta((x, y)))
        }
        // `Wheel`, `WheelTilt`, or an unreported source: prefer discrete detents.
        _ => {
            let (x120, y120) = frame.value120;
            let (xd, yd) = frame.discrete;
            if x120 != 0 || y120 != 0 {
                Some(ScrollDelta::LineDelta(
                    x120 as f32 / 120.0,
                    y120 as f32 / 120.0,
                ))
            } else if xd != 0 || yd != 0 {
                Some(ScrollDelta::LineDelta(xd as f32, yd as f32))
            } else {
                let (x, y) = frame.value;
                (x != 0.0 || y != 0.0).then(|| pixel_delta((x, y)))
            }
        }
    }
}

/// Offset added to a platform-provided pointer identifier before constructing a
/// [`PointerId`], so it can never collide with the reserved
/// [`PointerId::PRIMARY`] (whose underlying value is `1`).
///
/// Offsetting platform id `0` by `2` makes the first platform-derived id `2`.
pub const POINTER_ID_OFFSET: u64 = 2;

/// Build a non-primary [`PointerId`] from a platform identifier.
///
/// Applies [`POINTER_ID_OFFSET`] so the result never aliases
/// [`PointerId::PRIMARY`]. Returns `None` only on arithmetic overflow.
pub fn pointer_id_from_platform_id(id: u64) -> Option<PointerId> {
    id.checked_add(POINTER_ID_OFFSET).and_then(PointerId::new)
}

/// Build a [`PointerInfo`] for the primary pointer of the given [`PointerType`].
pub fn primary_pointer_info(pointer_type: PointerType) -> PointerInfo {
    PointerInfo {
        pointer_id: Some(PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type,
    }
}

/// Build a [`PointerInfo`] for a non-primary pointer of the given
/// [`PointerType`] from a platform identifier.
///
/// The identifier is offset through [`pointer_id_from_platform_id`] to avoid
/// colliding with [`PointerId::PRIMARY`].
pub fn pointer_info_from_platform_id(pointer_type: PointerType, id: u64) -> PointerInfo {
    PointerInfo {
        pointer_id: pointer_id_from_platform_id(id),
        persistent_device_id: None,
        pointer_type,
    }
}

/// Build a [`Modifiers`] set from individual modifier booleans.
///
/// Without an xkb keymap, `wl_keyboard` modifier state is an opaque bitmask, so
/// the keyboard reducer tracks these booleans from physical modifier key
/// presses and assembles them here.
pub fn modifiers_from_bools(ctrl: bool, alt: bool, shift: bool, meta: bool) -> Modifiers {
    let mut m = Modifiers::default();
    if ctrl {
        m.insert(Modifiers::CONTROL);
    }
    if alt {
        m.insert(Modifiers::ALT);
    }
    if shift {
        m.insert(Modifiers::SHIFT);
    }
    if meta {
        m.insert(Modifiers::META);
    }
    m
}

/// Map an evdev keyboard scancode to its physical [`Code`].
///
/// `wl_keyboard` reports each key by the Linux kernel's evdev scancode (the
/// `KEY_*` values from `linux/input-event-codes.h`); the same raw scancodes
/// appear in the `keys` array of a focus `enter` event. This maps that physical
/// position to the corresponding W3C UI Events [`Code`] for a standard PC
/// keyboard, independent of the active layout, mirroring the table the `winit`
/// adapter uses for X11 and Wayland. The Linux `KEY_LEFTMETA`/`KEY_RIGHTMETA`
/// keys map to [`Code::MetaLeft`]/[`Code::MetaRight`], the W3C names for the
/// operating-system ("super") keys.
///
/// To resolve the same scancode against an xkb keymap, add `8` to obtain the xkb
/// keycode; that path is handled under the `xkb` feature.
///
/// Scancodes with no standard physical key — including the rarer multimedia,
/// browser, and power keys — return [`Code::Unidentified`].
pub fn code_from_evdev_scancode(scancode: u32) -> Code {
    match scancode {
        // Main typing area: the alphanumeric block, punctuation, and the keys
        // that border it.
        1 => Code::Escape,
        2 => Code::Digit1,
        3 => Code::Digit2,
        4 => Code::Digit3,
        5 => Code::Digit4,
        6 => Code::Digit5,
        7 => Code::Digit6,
        8 => Code::Digit7,
        9 => Code::Digit8,
        10 => Code::Digit9,
        11 => Code::Digit0,
        12 => Code::Minus,
        13 => Code::Equal,
        14 => Code::Backspace,
        15 => Code::Tab,
        16 => Code::KeyQ,
        17 => Code::KeyW,
        18 => Code::KeyE,
        19 => Code::KeyR,
        20 => Code::KeyT,
        21 => Code::KeyY,
        22 => Code::KeyU,
        23 => Code::KeyI,
        24 => Code::KeyO,
        25 => Code::KeyP,
        26 => Code::BracketLeft,
        27 => Code::BracketRight,
        28 => Code::Enter,
        29 => Code::ControlLeft,
        30 => Code::KeyA,
        31 => Code::KeyS,
        32 => Code::KeyD,
        33 => Code::KeyF,
        34 => Code::KeyG,
        35 => Code::KeyH,
        36 => Code::KeyJ,
        37 => Code::KeyK,
        38 => Code::KeyL,
        39 => Code::Semicolon,
        40 => Code::Quote,
        41 => Code::Backquote,
        42 => Code::ShiftLeft,
        43 => Code::Backslash,
        44 => Code::KeyZ,
        45 => Code::KeyX,
        46 => Code::KeyC,
        47 => Code::KeyV,
        48 => Code::KeyB,
        49 => Code::KeyN,
        50 => Code::KeyM,
        51 => Code::Comma,
        52 => Code::Period,
        53 => Code::Slash,
        54 => Code::ShiftRight,
        55 => Code::NumpadMultiply,
        56 => Code::AltLeft,
        57 => Code::Space,
        58 => Code::CapsLock,
        // Function keys F1 through F10.
        59 => Code::F1,
        60 => Code::F2,
        61 => Code::F3,
        62 => Code::F4,
        63 => Code::F5,
        64 => Code::F6,
        65 => Code::F7,
        66 => Code::F8,
        67 => Code::F9,
        68 => Code::F10,
        // Locks and the numeric keypad.
        69 => Code::NumLock,
        70 => Code::ScrollLock,
        71 => Code::Numpad7,
        72 => Code::Numpad8,
        73 => Code::Numpad9,
        74 => Code::NumpadSubtract,
        75 => Code::Numpad4,
        76 => Code::Numpad5,
        77 => Code::Numpad6,
        78 => Code::NumpadAdd,
        79 => Code::Numpad1,
        80 => Code::Numpad2,
        81 => Code::Numpad3,
        82 => Code::Numpad0,
        83 => Code::NumpadDecimal,
        // International keys, the second function-key pair, and the editing and
        // navigation cluster.
        85 => Code::Lang5,
        86 => Code::IntlBackslash,
        87 => Code::F11,
        88 => Code::F12,
        89 => Code::IntlRo,
        90 => Code::Lang3,
        91 => Code::Lang4,
        92 => Code::Convert,
        93 => Code::KanaMode,
        94 => Code::NonConvert,
        96 => Code::NumpadEnter,
        97 => Code::ControlRight,
        98 => Code::NumpadDivide,
        99 => Code::PrintScreen,
        100 => Code::AltRight,
        102 => Code::Home,
        103 => Code::ArrowUp,
        104 => Code::PageUp,
        105 => Code::ArrowLeft,
        106 => Code::ArrowRight,
        107 => Code::End,
        108 => Code::ArrowDown,
        109 => Code::PageDown,
        110 => Code::Insert,
        111 => Code::Delete,
        113 => Code::AudioVolumeMute,
        114 => Code::AudioVolumeDown,
        115 => Code::AudioVolumeUp,
        117 => Code::NumpadEqual,
        119 => Code::Pause,
        121 => Code::NumpadComma,
        122 => Code::Lang1,
        123 => Code::Lang2,
        124 => Code::IntlYen,
        // The operating-system keys are W3C `Meta`, and the application/menu key.
        125 => Code::MetaLeft,
        126 => Code::MetaRight,
        127 => Code::ContextMenu,
        // A few common media keys and the high function-key range.
        163 => Code::MediaTrackNext,
        164 => Code::MediaPlayPause,
        165 => Code::MediaTrackPrevious,
        166 => Code::MediaStop,
        183 => Code::F13,
        184 => Code::F14,
        185 => Code::F15,
        186 => Code::F16,
        187 => Code::F17,
        188 => Code::F18,
        189 => Code::F19,
        190 => Code::F20,
        191 => Code::F21,
        192 => Code::F22,
        193 => Code::F23,
        194 => Code::F24,
        _ => Code::Unidentified,
    }
}

// XKB keysym values for the named (non-typing) keys, from
// `xkbcommon/xkbcommon-keysyms.h`. These are the frozen X11 keysym constants
// the `xkb` keymap resolves keys to; the typing keys resolve instead to Unicode
// or Latin-1 keysyms and carry their text, so they are not listed here.
const KEY_ISO_LEVEL3_SHIFT: u32 = 0xfe03;
const KEY_ISO_LEVEL3_LATCH: u32 = 0xfe04;
const KEY_ISO_LEVEL3_LOCK: u32 = 0xfe05;
const KEY_ISO_NEXT_GROUP: u32 = 0xfe08;
const KEY_ISO_PREV_GROUP: u32 = 0xfe0a;
const KEY_ISO_FIRST_GROUP: u32 = 0xfe0c;
const KEY_ISO_LAST_GROUP: u32 = 0xfe0e;
const KEY_ISO_LEFT_TAB: u32 = 0xfe20;
const KEY_ISO_ENTER: u32 = 0xfe34;
const KEY_BACKSPACE: u32 = 0xff08;
const KEY_TAB: u32 = 0xff09;
const KEY_CLEAR: u32 = 0xff0b;
const KEY_RETURN: u32 = 0xff0d;
const KEY_PAUSE: u32 = 0xff13;
const KEY_SCROLL_LOCK: u32 = 0xff14;
const KEY_SYS_REQ: u32 = 0xff15;
const KEY_ESCAPE: u32 = 0xff1b;
const KEY_MULTI_KEY: u32 = 0xff20;
const KEY_KANJI: u32 = 0xff21;
const KEY_MUHENKAN: u32 = 0xff22;
const KEY_HENKAN_MODE: u32 = 0xff23;
const KEY_ROMAJI: u32 = 0xff24;
const KEY_HIRAGANA: u32 = 0xff25;
const KEY_KATAKANA: u32 = 0xff26;
const KEY_HIRAGANA_KATAKANA: u32 = 0xff27;
const KEY_ZENKAKU: u32 = 0xff28;
const KEY_HANKAKU: u32 = 0xff29;
const KEY_ZENKAKU_HANKAKU: u32 = 0xff2a;
const KEY_KANA_LOCK: u32 = 0xff2d;
const KEY_EISU_TOGGLE: u32 = 0xff30;
const KEY_HANGUL: u32 = 0xff31;
const KEY_HANGUL_HANJA: u32 = 0xff34;
const KEY_HOME: u32 = 0xff50;
const KEY_LEFT: u32 = 0xff51;
const KEY_UP: u32 = 0xff52;
const KEY_RIGHT: u32 = 0xff53;
const KEY_DOWN: u32 = 0xff54;
const KEY_PAGE_UP: u32 = 0xff55;
const KEY_PAGE_DOWN: u32 = 0xff56;
const KEY_END: u32 = 0xff57;
const KEY_SELECT: u32 = 0xff60;
const KEY_PRINT: u32 = 0xff61;
const KEY_EXECUTE: u32 = 0xff62;
const KEY_INSERT: u32 = 0xff63;
const KEY_UNDO: u32 = 0xff65;
const KEY_REDO: u32 = 0xff66;
const KEY_MENU: u32 = 0xff67;
const KEY_FIND: u32 = 0xff68;
const KEY_CANCEL: u32 = 0xff69;
const KEY_HELP: u32 = 0xff6a;
const KEY_BREAK: u32 = 0xff6b;
const KEY_MODE_SWITCH: u32 = 0xff7e;
const KEY_NUM_LOCK: u32 = 0xff7f;
const KEY_KP_TAB: u32 = 0xff89;
const KEY_KP_ENTER: u32 = 0xff8d;
const KEY_KP_F1: u32 = 0xff91;
const KEY_KP_F2: u32 = 0xff92;
const KEY_KP_F3: u32 = 0xff93;
const KEY_KP_F4: u32 = 0xff94;
const KEY_KP_HOME: u32 = 0xff95;
const KEY_KP_LEFT: u32 = 0xff96;
const KEY_KP_UP: u32 = 0xff97;
const KEY_KP_RIGHT: u32 = 0xff98;
const KEY_KP_DOWN: u32 = 0xff99;
const KEY_KP_PAGE_UP: u32 = 0xff9a;
const KEY_KP_PAGE_DOWN: u32 = 0xff9b;
const KEY_KP_END: u32 = 0xff9c;
const KEY_KP_INSERT: u32 = 0xff9e;
const KEY_KP_DELETE: u32 = 0xff9f;
const KEY_F1: u32 = 0xffbe;
const KEY_F2: u32 = 0xffbf;
const KEY_F3: u32 = 0xffc0;
const KEY_F4: u32 = 0xffc1;
const KEY_F5: u32 = 0xffc2;
const KEY_F6: u32 = 0xffc3;
const KEY_F7: u32 = 0xffc4;
const KEY_F8: u32 = 0xffc5;
const KEY_F9: u32 = 0xffc6;
const KEY_F10: u32 = 0xffc7;
const KEY_F11: u32 = 0xffc8;
const KEY_F12: u32 = 0xffc9;
const KEY_F13: u32 = 0xffca;
const KEY_F14: u32 = 0xffcb;
const KEY_F15: u32 = 0xffcc;
const KEY_F16: u32 = 0xffcd;
const KEY_F17: u32 = 0xffce;
const KEY_F18: u32 = 0xffcf;
const KEY_F19: u32 = 0xffd0;
const KEY_F20: u32 = 0xffd1;
const KEY_F21: u32 = 0xffd2;
const KEY_F22: u32 = 0xffd3;
const KEY_F23: u32 = 0xffd4;
const KEY_F24: u32 = 0xffd5;
const KEY_F25: u32 = 0xffd6;
const KEY_F26: u32 = 0xffd7;
const KEY_F27: u32 = 0xffd8;
const KEY_F28: u32 = 0xffd9;
const KEY_F29: u32 = 0xffda;
const KEY_F30: u32 = 0xffdb;
const KEY_F31: u32 = 0xffdc;
const KEY_F32: u32 = 0xffdd;
const KEY_F33: u32 = 0xffde;
const KEY_F34: u32 = 0xffdf;
const KEY_F35: u32 = 0xffe0;
const KEY_SHIFT_L: u32 = 0xffe1;
const KEY_SHIFT_R: u32 = 0xffe2;
const KEY_CONTROL_L: u32 = 0xffe3;
const KEY_CONTROL_R: u32 = 0xffe4;
const KEY_CAPS_LOCK: u32 = 0xffe5;
const KEY_META_L: u32 = 0xffe7;
const KEY_META_R: u32 = 0xffe8;
const KEY_ALT_L: u32 = 0xffe9;
const KEY_ALT_R: u32 = 0xffea;
const KEY_SUPER_L: u32 = 0xffeb;
const KEY_SUPER_R: u32 = 0xffec;
const KEY_HYPER_L: u32 = 0xffed;
const KEY_HYPER_R: u32 = 0xffee;
const KEY_DELETE: u32 = 0xffff;

/// Map an XKB keysym to a W3C [`NamedKey`], if it names a non-typing key.
///
/// The `xkb` keymap resolves each key press to a keysym. The typing keys
/// (letters, digits, punctuation, and space) resolve to Unicode or Latin-1
/// keysyms and are better represented by the text they produce, so they return
/// `None` here and [`key_from_keysym`] falls back to that text. The remaining
/// keys — editing, navigation, function, modifier, lock, input-method, and
/// numeric-keypad action keys — map to their [`NamedKey`].
///
/// The keypad digit and operator keysyms also return `None`: with Num Lock on
/// they produce their character, and with it off the compositor reports the
/// matching navigation keysym (for example `KP_Left`) instead, which is named
/// here. Obscure multimedia, browser, and power keysyms return `None` too,
/// mirroring the keys [`code_from_evdev_scancode`] leaves [`Code::Unidentified`].
///
/// The Linux "super"/Windows-logo key and the legacy Hyper and Meta keys all
/// map to [`NamedKey::Meta`], the W3C name for the operating-system key.
pub fn named_key_from_keysym(keysym: u32) -> Option<NamedKey> {
    Some(match keysym {
        KEY_BACKSPACE => NamedKey::Backspace,
        KEY_TAB | KEY_KP_TAB | KEY_ISO_LEFT_TAB => NamedKey::Tab,
        KEY_CLEAR => NamedKey::Clear,
        KEY_RETURN | KEY_KP_ENTER | KEY_ISO_ENTER => NamedKey::Enter,
        KEY_PAUSE | KEY_BREAK => NamedKey::Pause,
        KEY_SCROLL_LOCK => NamedKey::ScrollLock,
        KEY_SYS_REQ | KEY_PRINT => NamedKey::PrintScreen,
        KEY_ESCAPE => NamedKey::Escape,
        KEY_DELETE | KEY_KP_DELETE => NamedKey::Delete,

        // Input-method and layout-group keys.
        KEY_MULTI_KEY => NamedKey::Compose,
        KEY_MODE_SWITCH => NamedKey::ModeChange,
        KEY_ISO_NEXT_GROUP => NamedKey::GroupNext,
        KEY_ISO_PREV_GROUP => NamedKey::GroupPrevious,
        KEY_ISO_FIRST_GROUP => NamedKey::GroupFirst,
        KEY_ISO_LAST_GROUP => NamedKey::GroupLast,

        // Japanese and Korean input keys.
        KEY_KANJI => NamedKey::KanjiMode,
        KEY_MUHENKAN => NamedKey::NonConvert,
        KEY_HENKAN_MODE => NamedKey::Convert,
        KEY_ROMAJI => NamedKey::Romaji,
        KEY_HIRAGANA => NamedKey::Hiragana,
        KEY_KATAKANA => NamedKey::Katakana,
        KEY_HIRAGANA_KATAKANA => NamedKey::HiraganaKatakana,
        KEY_ZENKAKU => NamedKey::Zenkaku,
        KEY_HANKAKU => NamedKey::Hankaku,
        KEY_ZENKAKU_HANKAKU => NamedKey::ZenkakuHankaku,
        KEY_KANA_LOCK => NamedKey::KanaMode,
        KEY_EISU_TOGGLE => NamedKey::Alphanumeric,
        KEY_HANGUL => NamedKey::HangulMode,
        KEY_HANGUL_HANJA => NamedKey::HanjaMode,

        // Navigation and editing, including the numeric-keypad equivalents the
        // keymap reports when Num Lock is off.
        KEY_HOME | KEY_KP_HOME => NamedKey::Home,
        KEY_LEFT | KEY_KP_LEFT => NamedKey::ArrowLeft,
        KEY_UP | KEY_KP_UP => NamedKey::ArrowUp,
        KEY_RIGHT | KEY_KP_RIGHT => NamedKey::ArrowRight,
        KEY_DOWN | KEY_KP_DOWN => NamedKey::ArrowDown,
        KEY_PAGE_UP | KEY_KP_PAGE_UP => NamedKey::PageUp,
        KEY_PAGE_DOWN | KEY_KP_PAGE_DOWN => NamedKey::PageDown,
        KEY_END | KEY_KP_END => NamedKey::End,
        KEY_INSERT | KEY_KP_INSERT => NamedKey::Insert,
        KEY_SELECT => NamedKey::Select,
        KEY_EXECUTE => NamedKey::Execute,
        KEY_UNDO => NamedKey::Undo,
        KEY_REDO => NamedKey::Redo,
        KEY_MENU => NamedKey::ContextMenu,
        KEY_FIND => NamedKey::Find,
        KEY_CANCEL => NamedKey::Cancel,
        KEY_HELP => NamedKey::Help,
        KEY_NUM_LOCK => NamedKey::NumLock,

        // Function keys, including the keypad's F1–F4.
        KEY_F1 | KEY_KP_F1 => NamedKey::F1,
        KEY_F2 | KEY_KP_F2 => NamedKey::F2,
        KEY_F3 | KEY_KP_F3 => NamedKey::F3,
        KEY_F4 | KEY_KP_F4 => NamedKey::F4,
        KEY_F5 => NamedKey::F5,
        KEY_F6 => NamedKey::F6,
        KEY_F7 => NamedKey::F7,
        KEY_F8 => NamedKey::F8,
        KEY_F9 => NamedKey::F9,
        KEY_F10 => NamedKey::F10,
        KEY_F11 => NamedKey::F11,
        KEY_F12 => NamedKey::F12,
        KEY_F13 => NamedKey::F13,
        KEY_F14 => NamedKey::F14,
        KEY_F15 => NamedKey::F15,
        KEY_F16 => NamedKey::F16,
        KEY_F17 => NamedKey::F17,
        KEY_F18 => NamedKey::F18,
        KEY_F19 => NamedKey::F19,
        KEY_F20 => NamedKey::F20,
        KEY_F21 => NamedKey::F21,
        KEY_F22 => NamedKey::F22,
        KEY_F23 => NamedKey::F23,
        KEY_F24 => NamedKey::F24,
        KEY_F25 => NamedKey::F25,
        KEY_F26 => NamedKey::F26,
        KEY_F27 => NamedKey::F27,
        KEY_F28 => NamedKey::F28,
        KEY_F29 => NamedKey::F29,
        KEY_F30 => NamedKey::F30,
        KEY_F31 => NamedKey::F31,
        KEY_F32 => NamedKey::F32,
        KEY_F33 => NamedKey::F33,
        KEY_F34 => NamedKey::F34,
        KEY_F35 => NamedKey::F35,

        // Modifier keys.
        KEY_SHIFT_L | KEY_SHIFT_R => NamedKey::Shift,
        KEY_CONTROL_L | KEY_CONTROL_R => NamedKey::Control,
        KEY_CAPS_LOCK => NamedKey::CapsLock,
        KEY_ALT_L | KEY_ALT_R => NamedKey::Alt,
        KEY_META_L | KEY_META_R | KEY_SUPER_L | KEY_SUPER_R | KEY_HYPER_L | KEY_HYPER_R => {
            NamedKey::Meta
        }
        KEY_ISO_LEVEL3_SHIFT | KEY_ISO_LEVEL3_LATCH | KEY_ISO_LEVEL3_LOCK => NamedKey::AltGraph,

        _ => return None,
    })
}

/// Resolve an XKB keysym and its typed text into a logical [`Key`].
///
/// The keysym is matched against [`named_key_from_keysym`] first, so the control
/// keys whose text would be a control character (such as Enter or Escape) are
/// named rather than treated as text. Otherwise non-empty `text` containing no
/// control characters becomes a [`Key::Character`] — this covers letters,
/// digits, punctuation, and the space key, which has no named-key form. Anything
/// else resolves to [`NamedKey::Unidentified`].
///
/// `text` is the string from `xkb_state_key_get_utf8` for the key in the current
/// keyboard state.
pub fn key_from_keysym(keysym: u32, text: &str) -> Key {
    if let Some(named) = named_key_from_keysym(keysym) {
        return Key::Named(named);
    }
    if !text.is_empty() && !text.chars().any(char::is_control) {
        return Key::Character(text.into());
    }
    Key::Named(NamedKey::Unidentified)
}

/// Map a physical [`Code`] to its W3C keyboard [`Location`].
///
/// The left and right variants of the modifier keys map to [`Location::Left`]
/// and [`Location::Right`], the numeric-keypad keys to [`Location::Numpad`], and
/// everything else — including `NumLock`, which the specification keeps on the
/// standard keyboard — to [`Location::Standard`].
pub fn location_from_code(code: Code) -> Location {
    match code {
        Code::ShiftLeft | Code::ControlLeft | Code::AltLeft | Code::MetaLeft => Location::Left,
        Code::ShiftRight | Code::ControlRight | Code::AltRight | Code::MetaRight => Location::Right,
        Code::Numpad0
        | Code::Numpad1
        | Code::Numpad2
        | Code::Numpad3
        | Code::Numpad4
        | Code::Numpad5
        | Code::Numpad6
        | Code::Numpad7
        | Code::Numpad8
        | Code::Numpad9
        | Code::NumpadAdd
        | Code::NumpadSubtract
        | Code::NumpadMultiply
        | Code::NumpadDivide
        | Code::NumpadDecimal
        | Code::NumpadComma
        | Code::NumpadEnter
        | Code::NumpadEqual => Location::Numpad,
        _ => Location::Standard,
    }
}

/// Assemble a [`Modifiers`] set from the individually active XKB modifiers.
///
/// This is the keymap-aware counterpart to [`modifiers_from_bools`]: it adds the
/// lock states and the Alt Graph (ISO level-3) modifier, which the keymap
/// resolves but the bare key stream cannot. Each argument is whether the
/// corresponding modifier is active, as reported by
/// `xkb_state_mod_name_is_active`.
pub fn modifiers_from_active_mods(
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
    caps_lock: bool,
    num_lock: bool,
    alt_graph: bool,
) -> Modifiers {
    let mut m = modifiers_from_bools(ctrl, alt, shift, meta);
    if caps_lock {
        m.insert(Modifiers::CAPS_LOCK);
    }
    if num_lock {
        m.insert(Modifiers::NUM_LOCK);
    }
    if alt_graph {
        m.insert(Modifiers::ALT_GRAPH);
    }
    m
}

/// Build a [`PointerInfo`] for a touch contact, reserving [`PointerId::PRIMARY`]
/// for the primary contact.
///
/// `wl_touch` identifies each contact with a small non-negative integer that is
/// unique among the contacts currently down. The contact whose id equals
/// `primary_id` — conventionally the lowest active id, mirroring the primary
/// pointer in the web backend — is mapped to [`PointerId::PRIMARY`]. Every other
/// contact is offset through [`pointer_info_from_platform_id`] so it cannot
/// collide with the reserved primary id.
pub fn touch_pointer_info(id: u64, primary_id: u64) -> PointerInfo {
    if id == primary_id {
        primary_pointer_info(PointerType::Touch)
    } else {
        pointer_info_from_platform_id(PointerType::Touch, id)
    }
}

/// Convert a `wl_touch` contact ellipse into a [`ContactGeometry`].
///
/// `wl_touch::shape` reports the lengths of the major and minor axes of the
/// ellipse approximating the contact, in surface-local (logical) coordinates.
/// Wayland aligns the major axis with the surface y-axis at the zero
/// orientation, so the major-axis length becomes the geometry's `height` and the
/// minor-axis length its `width`; the major-axis direction is reported
/// separately as the azimuth of [`touch_orientation_from_degrees`]. Both lengths
/// are multiplied by `scale_factor` to produce physical pixels.
///
/// A non-positive or non-finite axis length or `scale_factor` falls back to a
/// single physical pixel, matching the [`ContactGeometry`] default.
pub fn contact_geometry_from_shape(major: f64, minor: f64, scale_factor: f64) -> ContactGeometry {
    let scale_factor = positive_finite_or(scale_factor, 1.0);
    ContactGeometry {
        width: positive_finite_or(minor * scale_factor, 1.0),
        height: positive_finite_or(major * scale_factor, 1.0),
    }
}

/// Convert a `wl_touch` contact orientation into a [`PointerOrientation`].
///
/// `wl_touch::orientation` reports the clockwise angle, in degrees, between the
/// contact ellipse's major axis and the positive surface y-axis. A touch contact
/// lies flat against the surface, so the altitude is always perpendicular
/// (`π/2`); the angle is folded into the azimuth, which is `0` along the positive
/// x-axis and `π/2` along the positive y-axis. An orientation of `0°` therefore
/// yields the default [`PointerOrientation`].
///
/// A non-finite angle falls back to the default orientation.
pub fn touch_orientation_from_degrees(orientation_deg: f64) -> PointerOrientation {
    if !orientation_deg.is_finite() {
        return PointerOrientation::default();
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Wayland reports orientation as f64 degrees; ui-events stores azimuth as f32"
    )]
    let azimuth = (core::f32::consts::FRAC_PI_2 + (orientation_deg as f32).to_radians())
        .rem_euclid(core::f32::consts::TAU);
    PointerOrientation {
        altitude: core::f32::consts::FRAC_PI_2,
        azimuth,
    }
}

/// Convert a `zwp_pointer_gesture_pinch_v1` absolute pinch scale into the
/// signed, per-update scale fraction carried by [`PointerGesture::Pinch`].
///
/// Wayland reports the pinch `scale` as an absolute ratio relative to the
/// gesture's `begin` event, which is implicitly `1.0` — a `scale` of `2.0` means
/// the fingers are twice as far apart as they were at `begin`.
/// [`PointerGesture::Pinch`] instead carries the signed fractional change for a
/// single update, where `0.1` means "grow by 10%" and the update rule is
/// `new_scale = current_scale * (1.0 + delta)`. Given the absolute scale from
/// the previous update (or `1.0` at the start of the gesture) as `previous` and
/// the absolute scale from the current update as `current`, this returns
/// `current / previous - 1.0`.
///
/// A non-finite or non-positive `previous` falls back to `1.0`, and a
/// non-finite or non-positive `current` is treated as unchanged from `previous`,
/// yielding `0.0`.
///
/// [`PointerGesture::Pinch`]: ui_events::pointer::PointerGesture::Pinch
#[expect(
    clippy::cast_possible_truncation,
    reason = "Wayland reports scale as f64; ui-events stores the pinch fraction as f32"
)]
pub fn pinch_scale_fraction(previous: f64, current: f64) -> f32 {
    let previous = positive_finite_or(previous, 1.0);
    let current = positive_finite_or(current, previous);
    (current / previous - 1.0) as f32
}

/// Convert a `zwp_pointer_gesture_pinch_v1` rotation into the clockwise radians
/// carried by [`PointerGesture::Rotate`].
///
/// Wayland reports the pinch `rotation` as an angle in degrees, measured
/// clockwise, accumulated since the previous `begin` or `update` event — already
/// a per-update delta. [`PointerGesture::Rotate`] is likewise a clockwise,
/// per-update delta, but measured in radians, so this is a plain
/// degrees-to-radians conversion that preserves the sign. A non-finite angle
/// yields `0.0`.
///
/// [`PointerGesture::Rotate`]: ui_events::pointer::PointerGesture::Rotate
#[expect(
    clippy::cast_possible_truncation,
    reason = "Wayland reports rotation as f64 degrees; ui-events stores the angle as f32"
)]
pub fn rotation_radians_from_degrees(degrees: f64) -> f32 {
    finite_or(degrees, 0.0).to_radians() as f32
}

/// The physical type of a tablet tool, as reported by `zwp_tablet_tool_v2`.
///
/// This mirrors the protocol's `zwp_tablet_tool_v2::type` enum as plain values so
/// this module needs no `wayland-protocols` dependency; the tablet reducer
/// translates the protocol enum into this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolType {
    /// A pen.
    Pen,
    /// An eraser, conventionally the opposite end of a pen.
    Eraser,
    /// A brush.
    Brush,
    /// A pencil.
    Pencil,
    /// An airbrush.
    Airbrush,
    /// A finger on a touch-capable tablet.
    Finger,
    /// A mouse-shaped tool bound to the tablet surface (a puck).
    Mouse,
    /// A mouse-shaped tool with a focusing lens.
    Lens,
}

/// Map a tablet [`ToolType`] to the [`PointerType`] it reports as.
///
/// The pen-like drawing tools — pen, eraser, brush, pencil, and airbrush — report
/// as [`PointerType::Pen`]; the puck-style mouse and lens tools as
/// [`PointerType::Mouse`]; and the finger tool as [`PointerType::Touch`].
pub fn pointer_type_from_tool(tool: ToolType) -> PointerType {
    match tool {
        ToolType::Pen
        | ToolType::Eraser
        | ToolType::Brush
        | ToolType::Pencil
        | ToolType::Airbrush => PointerType::Pen,
        ToolType::Mouse | ToolType::Lens => PointerType::Mouse,
        ToolType::Finger => PointerType::Touch,
    }
}

/// The [`PointerButton`] a tablet tool's tip contact maps to.
///
/// A [`ToolType::Eraser`] tip maps to [`PointerButton::PenEraser`], matching the
/// W3C Pointer Events eraser button; every other tool's tip maps to
/// [`PointerButton::Primary`], the pen-contact / primary button.
pub fn tip_button_from_tool(tool: ToolType) -> PointerButton {
    match tool {
        ToolType::Eraser => PointerButton::PenEraser,
        _ => PointerButton::Primary,
    }
}

/// The maximum value of a tablet tool's normalized axes (`pressure`, `distance`,
/// and `slider`), per `zwp_tablet_tool_v2`.
const TABLET_AXIS_NORMAL_MAX: f32 = 65535.0;

/// Convert a `zwp_tablet_tool_v2::pressure` value into a normalized pressure.
///
/// Wayland reports tool pressure as an integer normalized to `[0, 65535]`. This
/// divides by that maximum to produce the `[0, 1]` pressure
/// [`PointerState`] expects, clamping to guard against out-of-range values.
///
/// [`PointerState`]: ui_events::pointer::PointerState
pub fn pressure_from_normalized(value: u32) -> f32 {
    (value as f32 / TABLET_AXIS_NORMAL_MAX).clamp(0.0, 1.0)
}

/// Convert a `zwp_tablet_tool_v2::slider` position into a tangential pressure.
///
/// Wayland reports the slider as a signed integer normalized to
/// `[-65535, 65535]`. This divides by `65535` to produce the `[-1, 1]`
/// tangential pressure [`PointerState`] expects, clamping to guard against
/// out-of-range values. A tool slider is one of the controls
/// [`PointerState::tangential_pressure`] is intended to carry.
///
/// [`PointerState`]: ui_events::pointer::PointerState
/// [`PointerState::tangential_pressure`]: ui_events::pointer::PointerState::tangential_pressure
pub fn tangential_pressure_from_slider(position: i32) -> f32 {
    (position as f32 / TABLET_AXIS_NORMAL_MAX).clamp(-1.0, 1.0)
}

/// The tilt magnitude, in degrees, at which the pen is treated as parallel to the
/// surface, kept just below `90°` so `tan` stays finite.
const MAX_TILT_DEGREES: f32 = 89.9;

/// Convert `zwp_tablet_tool_v2::tilt` angles into a [`PointerOrientation`].
///
/// Wayland reports the tilt as two angles in degrees: `tilt_x` is the deflection
/// of the tool away from the surface normal toward the positive surface x-axis,
/// and `tilt_y` toward the positive y-axis, each nominally in `[-90, 90]`. This
/// matches the W3C Pointer Events `tiltX`/`tiltY` model, so the conversion mirrors
/// the web backend: the pen axis is modelled as the vector `(tan(tilt_x),
/// tan(tilt_y), 1)`, whose spherical altitude and azimuth become the
/// orientation. A zero tilt yields the perpendicular default orientation.
///
/// The angles are clamped to just under `90°` so the tangents stay finite, and a
/// non-finite angle is treated as `0`.
pub fn pointer_orientation_from_tilt_degrees(
    tilt_x_deg: f64,
    tilt_y_deg: f64,
) -> PointerOrientation {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Wayland reports tilt as f64 degrees; ui-events stores orientation as f32"
    )]
    let (tilt_x, tilt_y) = (
        (finite_or(tilt_x_deg, 0.0) as f32).clamp(-MAX_TILT_DEGREES, MAX_TILT_DEGREES),
        (finite_or(tilt_y_deg, 0.0) as f32).clamp(-MAX_TILT_DEGREES, MAX_TILT_DEGREES),
    );
    let x = tilt_x.to_radians().tan();
    let y = tilt_y.to_radians().tan();

    // The pen axis is the normalized vector (x, y, 1); its z component is the
    // sine of the altitude, and its projection onto the surface gives the azimuth.
    let z = 1.0 / (x.mul_add(x, y * y) + 1.0).sqrt();
    let altitude = z.asin();
    let azimuth = if x == 0.0 && y == 0.0 {
        core::f32::consts::FRAC_PI_2
    } else {
        y.atan2(x)
    };
    PointerOrientation { altitude, azimuth }
}

#[inline]
fn finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() { value } else { fallback }
}

#[inline]
fn positive_finite_or(value: f64, fallback: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evdev_buttons_map_to_expected_pointer_buttons() {
        assert_eq!(
            pointer_button_from_evdev(BTN_LEFT),
            Some(PointerButton::Primary)
        );
        assert_eq!(
            pointer_button_from_evdev(BTN_RIGHT),
            Some(PointerButton::Secondary)
        );
        assert_eq!(
            pointer_button_from_evdev(BTN_MIDDLE),
            Some(PointerButton::Auxiliary)
        );
        assert_eq!(pointer_button_from_evdev(BTN_SIDE), Some(PointerButton::X1));
        assert_eq!(
            pointer_button_from_evdev(BTN_EXTRA),
            Some(PointerButton::X2)
        );
        assert_eq!(
            pointer_button_from_evdev(BTN_FORWARD),
            Some(PointerButton::B7)
        );
        assert_eq!(pointer_button_from_evdev(BTN_BACK), Some(PointerButton::B8));
        assert_eq!(pointer_button_from_evdev(BTN_TASK), Some(PointerButton::B9));
    }

    #[test]
    fn evdev_unknown_button_is_none() {
        // Just below `BTN_LEFT`, just above `BTN_TASK`, and `BTN_TOUCH`.
        assert_eq!(pointer_button_from_evdev(0x10f), None);
        assert_eq!(pointer_button_from_evdev(0x118), None);
        assert_eq!(pointer_button_from_evdev(0x14a), None);
    }

    #[test]
    fn logical_coordinates_scale_to_physical_pixels() {
        assert_eq!(
            physical_position_from_logical(10.0, 20.0, 2.0),
            PhysicalPosition { x: 20.0, y: 40.0 }
        );
    }

    #[test]
    fn position_sanitizes_non_finite_inputs() {
        // Non-finite scale falls back to 1.0; non-finite coordinate to 0.0.
        assert_eq!(
            physical_position_from_logical(f64::NAN, 5.0, f64::INFINITY),
            PhysicalPosition { x: 0.0, y: 5.0 }
        );
    }

    #[test]
    fn wheel_frame_prefers_value120_over_discrete() {
        let frame = AxisFrame {
            source: Some(AxisSource::Wheel),
            value: (0.0, 30.0),
            value120: (0, 240),
            discrete: (0, 2),
        };
        assert_eq!(
            scroll_delta_from_axis_frame(frame, 2.0),
            Some(ScrollDelta::LineDelta(0.0, 2.0))
        );
    }

    #[test]
    fn wheel_frame_falls_back_to_discrete() {
        let frame = AxisFrame {
            source: Some(AxisSource::Wheel),
            discrete: (0, -1),
            ..Default::default()
        };
        assert_eq!(
            scroll_delta_from_axis_frame(frame, 1.0),
            Some(ScrollDelta::LineDelta(0.0, -1.0))
        );
    }

    #[test]
    fn finger_frame_is_pixel_delta_scaled() {
        let frame = AxisFrame {
            source: Some(AxisSource::Finger),
            value: (3.0, -4.0),
            ..Default::default()
        };
        assert_eq!(
            scroll_delta_from_axis_frame(frame, 2.0),
            Some(ScrollDelta::PixelDelta(PhysicalPosition {
                x: 6.0,
                y: -8.0
            }))
        );
    }

    #[test]
    fn empty_frame_is_none() {
        assert_eq!(
            scroll_delta_from_axis_frame(AxisFrame::default(), 1.0),
            None
        );
    }

    #[test]
    fn sourceless_frame_with_only_value_is_pixel_delta() {
        let frame = AxisFrame {
            source: None,
            value: (0.0, 12.0),
            ..Default::default()
        };
        assert_eq!(
            scroll_delta_from_axis_frame(frame, 1.0),
            Some(ScrollDelta::PixelDelta(PhysicalPosition {
                x: 0.0,
                y: 12.0
            }))
        );
    }

    #[test]
    fn platform_pointer_id_does_not_collide_with_primary() {
        let id = pointer_id_from_platform_id(0).expect("offset id should be valid");
        assert_eq!(id.get_inner().get(), POINTER_ID_OFFSET);
        assert_ne!(id, PointerId::PRIMARY);
    }

    #[test]
    fn primary_pointer_info_is_primary() {
        let info = primary_pointer_info(PointerType::Mouse);
        assert!(info.is_primary_pointer());
        assert_eq!(info.pointer_type, PointerType::Mouse);
    }

    #[test]
    fn platform_pointer_info_is_not_primary() {
        let info = pointer_info_from_platform_id(PointerType::Touch, 0);
        assert!(!info.is_primary_pointer());
        assert_eq!(info.pointer_type, PointerType::Touch);
        assert_eq!(info.pointer_id, PointerId::new(POINTER_ID_OFFSET));
    }

    #[test]
    fn modifiers_from_bools_sets_expected_bits() {
        let mods = modifiers_from_bools(true, false, true, false);
        assert!(mods.ctrl());
        assert!(!mods.alt());
        assert!(mods.shift());
        assert!(!mods.meta());
    }

    #[test]
    fn evdev_scancodes_map_to_expected_physical_codes() {
        // Representative keys; the `KEY_*` values are from
        // `linux/input-event-codes.h`.
        assert_eq!(code_from_evdev_scancode(30), Code::KeyA);
        assert_eq!(code_from_evdev_scancode(1), Code::Escape);
        assert_eq!(code_from_evdev_scancode(28), Code::Enter);
        assert_eq!(code_from_evdev_scancode(57), Code::Space);
        // The top-row digits are distinct from the numeric keypad.
        assert_eq!(code_from_evdev_scancode(2), Code::Digit1);
        assert_eq!(code_from_evdev_scancode(11), Code::Digit0);
        assert_eq!(code_from_evdev_scancode(82), Code::Numpad0);
        assert_eq!(code_from_evdev_scancode(96), Code::NumpadEnter);
        // Function keys span a low and a high range.
        assert_eq!(code_from_evdev_scancode(59), Code::F1);
        assert_eq!(code_from_evdev_scancode(88), Code::F12);
        assert_eq!(code_from_evdev_scancode(183), Code::F13);
        assert_eq!(code_from_evdev_scancode(194), Code::F24);
    }

    #[test]
    fn evdev_modifier_scancodes_map_to_sided_codes() {
        assert_eq!(code_from_evdev_scancode(29), Code::ControlLeft);
        assert_eq!(code_from_evdev_scancode(97), Code::ControlRight);
        assert_eq!(code_from_evdev_scancode(42), Code::ShiftLeft);
        assert_eq!(code_from_evdev_scancode(54), Code::ShiftRight);
        assert_eq!(code_from_evdev_scancode(56), Code::AltLeft);
        assert_eq!(code_from_evdev_scancode(100), Code::AltRight);
        // The Linux meta/super keys are the W3C Meta keys.
        assert_eq!(code_from_evdev_scancode(125), Code::MetaLeft);
        assert_eq!(code_from_evdev_scancode(126), Code::MetaRight);
    }

    #[test]
    fn unmapped_evdev_scancodes_are_unidentified() {
        // `0` is reserved, `84` is a gap in the table, `116` (`KEY_POWER`) and
        // the rarer multimedia keys are intentionally unmapped, and very large
        // values have no key.
        assert_eq!(code_from_evdev_scancode(0), Code::Unidentified);
        assert_eq!(code_from_evdev_scancode(84), Code::Unidentified);
        assert_eq!(code_from_evdev_scancode(116), Code::Unidentified);
        assert_eq!(code_from_evdev_scancode(u32::MAX), Code::Unidentified);
    }

    #[test]
    fn touch_primary_contact_is_primary_pointer() {
        let info = touch_pointer_info(3, 3);
        assert!(info.is_primary_pointer());
        assert_eq!(info.pointer_type, PointerType::Touch);
    }

    #[test]
    fn touch_non_primary_contact_is_offset() {
        let info = touch_pointer_info(3, 1);
        assert!(!info.is_primary_pointer());
        assert_eq!(info.pointer_type, PointerType::Touch);
        assert_eq!(info.pointer_id, PointerId::new(3 + POINTER_ID_OFFSET));
    }

    #[test]
    fn shape_maps_minor_to_width_and_major_to_height_scaled() {
        let geometry = contact_geometry_from_shape(10.0, 4.0, 2.0);
        assert_eq!(geometry.width, 8.0);
        assert_eq!(geometry.height, 20.0);
    }

    #[test]
    fn shape_sanitizes_degenerate_axes() {
        let geometry = contact_geometry_from_shape(0.0, f64::NAN, 1.0);
        assert_eq!(geometry.width, 1.0);
        assert_eq!(geometry.height, 1.0);
    }

    #[test]
    fn orientation_zero_degrees_is_default() {
        let orientation = touch_orientation_from_degrees(0.0);
        assert_eq!(orientation.altitude, core::f32::consts::FRAC_PI_2);
        assert!((orientation.azimuth - core::f32::consts::FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn orientation_rotates_azimuth_clockwise_from_y_axis() {
        // 90° clockwise from the +y axis puts the major axis along azimuth π.
        let orientation = touch_orientation_from_degrees(90.0);
        assert!((orientation.azimuth - core::f32::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn orientation_non_finite_is_default() {
        assert_eq!(
            touch_orientation_from_degrees(f64::INFINITY),
            PointerOrientation::default()
        );
    }

    #[test]
    fn pinch_growth_is_a_positive_fraction() {
        // Growing from the 1.0 begin baseline to 1.1 is a 10% increase.
        assert!((pinch_scale_fraction(1.0, 1.1) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn pinch_shrink_is_a_negative_fraction() {
        assert!((pinch_scale_fraction(1.0, 0.5) + 0.5).abs() < 1e-6);
    }

    #[test]
    fn pinch_fraction_is_relative_to_the_previous_scale() {
        // Two updates each growing the absolute scale by 10% each report ~10%,
        // because the protocol's scale is absolute, not per-update.
        assert!((pinch_scale_fraction(1.1, 1.21) - 0.1).abs() < 1e-5);
    }

    #[test]
    fn pinch_fraction_sanitizes_degenerate_scales() {
        // Non-positive previous falls back to 1.0; non-finite current is treated
        // as no change.
        assert_eq!(pinch_scale_fraction(0.0, 1.0), 0.0);
        assert_eq!(pinch_scale_fraction(1.0, f64::NAN), 0.0);
    }

    #[test]
    fn rotation_converts_degrees_to_clockwise_radians() {
        assert!((rotation_radians_from_degrees(90.0) - core::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert!((rotation_radians_from_degrees(-45.0) + core::f32::consts::FRAC_PI_4).abs() < 1e-6);
    }

    #[test]
    fn rotation_non_finite_is_zero() {
        assert_eq!(rotation_radians_from_degrees(f64::INFINITY), 0.0);
    }

    #[test]
    fn pen_buttons_map_barrel_and_defer_to_pointer_table() {
        // The primary barrel button is the documented pen "Secondary" button.
        assert_eq!(
            pen_button_from_evdev(BTN_STYLUS),
            Some(PointerButton::Secondary)
        );
        // Extra barrel buttons fall into the generic range.
        assert_eq!(pen_button_from_evdev(BTN_STYLUS2), Some(PointerButton::B7));
        assert_eq!(pen_button_from_evdev(BTN_STYLUS3), Some(PointerButton::B8));
        // A puck tool's mouse buttons defer to the pointer-button table.
        assert_eq!(
            pen_button_from_evdev(BTN_LEFT),
            Some(PointerButton::Primary)
        );
        // `BTN_TOUCH` (0x14a) sits between the stylus codes and maps to nothing.
        assert_eq!(pen_button_from_evdev(0x14a), None);
    }

    #[test]
    fn tool_types_map_to_expected_pointer_types() {
        assert_eq!(pointer_type_from_tool(ToolType::Pen), PointerType::Pen);
        assert_eq!(pointer_type_from_tool(ToolType::Eraser), PointerType::Pen);
        assert_eq!(pointer_type_from_tool(ToolType::Airbrush), PointerType::Pen);
        assert_eq!(pointer_type_from_tool(ToolType::Mouse), PointerType::Mouse);
        assert_eq!(pointer_type_from_tool(ToolType::Lens), PointerType::Mouse);
        assert_eq!(pointer_type_from_tool(ToolType::Finger), PointerType::Touch);
    }

    #[test]
    fn eraser_tip_is_pen_eraser_others_are_primary() {
        assert_eq!(
            tip_button_from_tool(ToolType::Eraser),
            PointerButton::PenEraser
        );
        assert_eq!(tip_button_from_tool(ToolType::Pen), PointerButton::Primary);
        assert_eq!(
            tip_button_from_tool(ToolType::Mouse),
            PointerButton::Primary
        );
    }

    #[test]
    fn pressure_normalizes_and_clamps() {
        assert_eq!(pressure_from_normalized(0), 0.0);
        assert_eq!(pressure_from_normalized(65535), 1.0);
        assert!((pressure_from_normalized(32768) - 0.5).abs() < 1e-4);
        // Out-of-range values clamp into [0, 1].
        assert_eq!(pressure_from_normalized(u32::MAX), 1.0);
    }

    #[test]
    fn slider_normalizes_signed_and_clamps() {
        assert_eq!(tangential_pressure_from_slider(0), 0.0);
        assert_eq!(tangential_pressure_from_slider(65535), 1.0);
        assert_eq!(tangential_pressure_from_slider(-65535), -1.0);
        assert!((tangential_pressure_from_slider(32768) - 0.5).abs() < 1e-4);
        // Out-of-range values clamp into [-1, 1].
        assert_eq!(tangential_pressure_from_slider(i32::MIN), -1.0);
    }

    #[test]
    fn zero_tilt_is_perpendicular() {
        let orientation = pointer_orientation_from_tilt_degrees(0.0, 0.0);
        assert!((orientation.altitude - core::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!((orientation.azimuth - core::f32::consts::FRAC_PI_2).abs() < 1e-5);
    }

    #[test]
    fn tilt_axes_map_to_expected_azimuths() {
        // Tilt toward +x points the azimuth along the x-axis (0).
        let toward_x = pointer_orientation_from_tilt_degrees(30.0, 0.0);
        assert!(toward_x.azimuth.abs() < 1e-5);
        // Tilt toward +y points the azimuth along the y-axis (π/2).
        let toward_y = pointer_orientation_from_tilt_degrees(0.0, 30.0);
        assert!((toward_y.azimuth - core::f32::consts::FRAC_PI_2).abs() < 1e-5);
        // More tilt lowers the altitude toward the surface.
        assert!(toward_x.altitude < core::f32::consts::FRAC_PI_2);
    }

    #[test]
    fn extreme_tilt_stays_finite() {
        // Beyond the clamp the tangents would diverge; the result must stay finite.
        let orientation = pointer_orientation_from_tilt_degrees(120.0, -120.0);
        assert!(orientation.altitude.is_finite());
        assert!(orientation.azimuth.is_finite());
        assert!(orientation.altitude < 0.01);
    }

    #[test]
    fn non_finite_tilt_is_perpendicular() {
        let orientation = pointer_orientation_from_tilt_degrees(f64::NAN, f64::INFINITY);
        assert!((orientation.altitude - core::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!((orientation.azimuth - core::f32::consts::FRAC_PI_2).abs() < 1e-5);
    }

    #[test]
    fn keysym_named_keys_map_to_expected_named_keys() {
        assert_eq!(named_key_from_keysym(KEY_ESCAPE), Some(NamedKey::Escape));
        assert_eq!(named_key_from_keysym(KEY_RETURN), Some(NamedKey::Enter));
        assert_eq!(named_key_from_keysym(KEY_KP_ENTER), Some(NamedKey::Enter));
        assert_eq!(
            named_key_from_keysym(KEY_BACKSPACE),
            Some(NamedKey::Backspace)
        );
        assert_eq!(named_key_from_keysym(KEY_F1), Some(NamedKey::F1));
        assert_eq!(named_key_from_keysym(KEY_F24), Some(NamedKey::F24));
        assert_eq!(named_key_from_keysym(KEY_LEFT), Some(NamedKey::ArrowLeft));
        // The numpad arrow (Num Lock off) resolves to the same logical key.
        assert_eq!(
            named_key_from_keysym(KEY_KP_LEFT),
            Some(NamedKey::ArrowLeft)
        );
        assert_eq!(named_key_from_keysym(KEY_SHIFT_L), Some(NamedKey::Shift));
        assert_eq!(named_key_from_keysym(KEY_SHIFT_R), Some(NamedKey::Shift));
        // The super/logo and legacy meta keys all fold into Meta.
        assert_eq!(named_key_from_keysym(KEY_SUPER_L), Some(NamedKey::Meta));
        assert_eq!(named_key_from_keysym(KEY_META_R), Some(NamedKey::Meta));
        assert_eq!(
            named_key_from_keysym(KEY_ISO_LEVEL3_SHIFT),
            Some(NamedKey::AltGraph)
        );
        assert_eq!(named_key_from_keysym(KEY_NUM_LOCK), Some(NamedKey::NumLock));
    }

    #[test]
    fn typing_keysyms_are_not_named() {
        // Letters, digits, and space resolve to text, not a named key.
        assert_eq!(named_key_from_keysym(0x61), None); // 'a'
        assert_eq!(named_key_from_keysym(0x41), None); // 'A'
        assert_eq!(named_key_from_keysym(0x31), None); // '1'
        assert_eq!(named_key_from_keysym(0x20), None); // space
        assert_eq!(named_key_from_keysym(0), None); // NoSymbol
    }

    #[test]
    fn key_from_keysym_prefers_named_then_text() {
        // A named key wins even though xkb hands back a control character for it.
        assert_eq!(
            key_from_keysym(KEY_ESCAPE, "\u{1b}"),
            Key::Named(NamedKey::Escape)
        );
        // Printable text becomes a character key.
        assert_eq!(key_from_keysym(0x61, "a"), Key::Character("a".into()));
        // Space has no named-key form, so it comes through as its character.
        assert_eq!(key_from_keysym(0x20, " "), Key::Character(" ".into()));
        // Bare control text with no named key is unidentified, not a character.
        assert_eq!(
            key_from_keysym(0, "\u{1b}"),
            Key::Named(NamedKey::Unidentified)
        );
        assert_eq!(key_from_keysym(0, ""), Key::Named(NamedKey::Unidentified));
    }

    #[test]
    fn location_from_code_classifies_sides_and_numpad() {
        assert_eq!(location_from_code(Code::ShiftLeft), Location::Left);
        assert_eq!(location_from_code(Code::ControlRight), Location::Right);
        assert_eq!(location_from_code(Code::MetaLeft), Location::Left);
        assert_eq!(location_from_code(Code::Numpad5), Location::Numpad);
        assert_eq!(location_from_code(Code::NumpadEnter), Location::Numpad);
        assert_eq!(location_from_code(Code::KeyA), Location::Standard);
        // Num Lock itself stays on the standard keyboard.
        assert_eq!(location_from_code(Code::NumLock), Location::Standard);
    }

    #[test]
    fn modifiers_from_active_mods_adds_locks_and_alt_graph() {
        let mods = modifiers_from_active_mods(true, false, true, false, true, false, true);
        assert!(mods.contains(Modifiers::CONTROL));
        assert!(mods.contains(Modifiers::SHIFT));
        assert!(mods.contains(Modifiers::CAPS_LOCK));
        assert!(mods.contains(Modifiers::ALT_GRAPH));
        assert!(!mods.contains(Modifiers::ALT));
        assert!(!mods.contains(Modifiers::META));
        assert!(!mods.contains(Modifiers::NUM_LOCK));
    }

    /// Cross-check the hand-copied keysym constants against the libxkbcommon
    /// definitions, so a transcription error in the table is caught. This needs
    /// the `xkbcommon` crate, so it is gated to a Linux build with the `xkb`
    /// feature.
    #[cfg(all(target_os = "linux", feature = "xkb"))]
    #[test]
    fn keysym_constants_match_xkbcommon() {
        use xkbcommon::xkb::keysyms;
        assert_eq!(KEY_BACKSPACE, keysyms::KEY_BackSpace);
        assert_eq!(KEY_TAB, keysyms::KEY_Tab);
        assert_eq!(KEY_RETURN, keysyms::KEY_Return);
        assert_eq!(KEY_ESCAPE, keysyms::KEY_Escape);
        assert_eq!(KEY_DELETE, keysyms::KEY_Delete);
        assert_eq!(KEY_HOME, keysyms::KEY_Home);
        assert_eq!(KEY_LEFT, keysyms::KEY_Left);
        assert_eq!(KEY_END, keysyms::KEY_End);
        assert_eq!(KEY_KP_ENTER, keysyms::KEY_KP_Enter);
        assert_eq!(KEY_KP_LEFT, keysyms::KEY_KP_Left);
        assert_eq!(KEY_NUM_LOCK, keysyms::KEY_Num_Lock);
        assert_eq!(KEY_F1, keysyms::KEY_F1);
        assert_eq!(KEY_F12, keysyms::KEY_F12);
        assert_eq!(KEY_F35, keysyms::KEY_F35);
        assert_eq!(KEY_SHIFT_L, keysyms::KEY_Shift_L);
        assert_eq!(KEY_CONTROL_R, keysyms::KEY_Control_R);
        assert_eq!(KEY_SUPER_L, keysyms::KEY_Super_L);
        assert_eq!(KEY_ALT_R, keysyms::KEY_Alt_R);
        assert_eq!(KEY_ISO_LEVEL3_SHIFT, keysyms::KEY_ISO_Level3_Shift);
        assert_eq!(KEY_ISO_LEFT_TAB, keysyms::KEY_ISO_Left_Tab);
    }
}
