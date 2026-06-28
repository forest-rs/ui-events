// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Support routines for converting keyboard data from the raw Win32 API.
//!
//! The scancode and virtual-key tables in this module are inlined from
//! `winit-win32`'s `keyboard.rs` and `keyboard_layout.rs`, with the
//! intermediate `winit` types replaced by the equivalent
//! [`ui_events::keyboard`] types. Unlike `winit-win32`, this module does not
//! reproduce the full `WM_KEYDOWN`/`WM_DEADCHAR`/`WM_CHAR` sequencing dance;
//! instead, it derives the logical key for a single `WM_KEYDOWN`/`WM_KEYUP`
//! message directly via `ToUnicode`. Multi-keystroke dead-key composition is
//! therefore simplified relative to upstream `winit`.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use ui_events::keyboard::{Code, Key, KeyState, KeyboardEvent, Location, Modifiers, NamedKey};
use windows_sys::Win32::Foundation::{LPARAM, WPARAM};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, GetKeyboardState, MAPVK_VK_TO_VSC, MapVirtualKeyW, ToUnicode, VIRTUAL_KEY,
    VK_ABNT_C2, VK_ADD, VK_CLEAR, VK_CONTROL, VK_DECIMAL, VK_DELETE, VK_DIVIDE, VK_DOWN, VK_END,
    VK_HOME, VK_INSERT, VK_LCONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_MULTIPLY,
    VK_NEXT, VK_NUMPAD0, VK_NUMPAD1, VK_NUMPAD2, VK_NUMPAD3, VK_NUMPAD4, VK_NUMPAD5, VK_NUMPAD6,
    VK_NUMPAD7, VK_NUMPAD8, VK_NUMPAD9, VK_PRIOR, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU,
    VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_SUBTRACT, VK_UP,
};

/// Extracted fields of the `lParam` of a `WM_KEYDOWN`/`WM_KEYUP`/`WM_SYSKEYDOWN`/`WM_SYSKEYUP`
/// message.
///
/// Inlined from `winit-win32`'s `destructure_key_lparam`.
struct KeyLParam {
    scancode: u8,
    extended: bool,
    is_repeat: bool,
}

fn destructure_key_lparam(lparam: LPARAM) -> KeyLParam {
    let previous_state = (lparam >> 30) & 0x01;
    let transition_state = (lparam >> 31) & 0x01;
    KeyLParam {
        scancode: ((lparam >> 16) & 0xff) as u8,
        extended: ((lparam >> 24) & 0x01) != 0,
        is_repeat: (previous_state ^ transition_state) != 0,
    }
}

/// Combine a raw scancode byte and the lParam's extended flag into the
/// "extended scancode" used by [`scancode_to_code`].
///
/// Inlined from `winit-win32`'s `new_ex_scancode`.
fn new_ex_scancode(scancode: u8, extended: bool) -> u16 {
    (scancode as u16) | (if extended { 0xe000 } else { 0 })
}

/// Convert a Win32 extended scancode into a [`Code`].
///
/// Inlined from `winit-win32`'s `scancode_to_physicalkey`, with
/// `winit_core::keyboard::KeyCode` variants replaced by the equivalent
/// [`ui_events::keyboard::Code`] variants (the two enums share the same
/// names, both following the W3C UI Events Code spec).
pub fn scancode_to_code(scancode: u32) -> Code {
    match scancode {
        0x0029 => Code::Backquote,
        0x002b => Code::Backslash,
        0x000e => Code::Backspace,
        0x001a => Code::BracketLeft,
        0x001b => Code::BracketRight,
        0x0033 => Code::Comma,
        0x000b => Code::Digit0,
        0x0002 => Code::Digit1,
        0x0003 => Code::Digit2,
        0x0004 => Code::Digit3,
        0x0005 => Code::Digit4,
        0x0006 => Code::Digit5,
        0x0007 => Code::Digit6,
        0x0008 => Code::Digit7,
        0x0009 => Code::Digit8,
        0x000a => Code::Digit9,
        0x000d => Code::Equal,
        0x0056 => Code::IntlBackslash,
        0x0073 => Code::IntlRo,
        0x007d => Code::IntlYen,
        0x001e => Code::KeyA,
        0x0030 => Code::KeyB,
        0x002e => Code::KeyC,
        0x0020 => Code::KeyD,
        0x0012 => Code::KeyE,
        0x0021 => Code::KeyF,
        0x0022 => Code::KeyG,
        0x0023 => Code::KeyH,
        0x0017 => Code::KeyI,
        0x0024 => Code::KeyJ,
        0x0025 => Code::KeyK,
        0x0026 => Code::KeyL,
        0x0032 => Code::KeyM,
        0x0031 => Code::KeyN,
        0x0018 => Code::KeyO,
        0x0019 => Code::KeyP,
        0x0010 => Code::KeyQ,
        0x0013 => Code::KeyR,
        0x001f => Code::KeyS,
        0x0014 => Code::KeyT,
        0x0016 => Code::KeyU,
        0x002f => Code::KeyV,
        0x0011 => Code::KeyW,
        0x002d => Code::KeyX,
        0x0015 => Code::KeyY,
        0x002c => Code::KeyZ,
        0x000c => Code::Minus,
        0x0034 => Code::Period,
        0x0028 => Code::Quote,
        0x0027 => Code::Semicolon,
        0x0035 => Code::Slash,
        0x0038 => Code::AltLeft,
        0xe038 => Code::AltRight,
        0x003a => Code::CapsLock,
        0xe05d => Code::ContextMenu,
        0x001d => Code::ControlLeft,
        0xe01d => Code::ControlRight,
        0x001c => Code::Enter,
        0xe05b => Code::MetaLeft,
        0xe05c => Code::MetaRight,
        0x002a => Code::ShiftLeft,
        0x0036 => Code::ShiftRight,
        0x0039 => Code::Space,
        0x000f => Code::Tab,
        0x0079 => Code::Convert,
        0x0072 => Code::Lang1, // for non-Korean layout
        0xe0f2 => Code::Lang1, // for Korean layout
        0x0071 => Code::Lang2, // for non-Korean layout
        0xe0f1 => Code::Lang2, // for Korean layout
        0x0070 => Code::KanaMode,
        0x007b => Code::NonConvert,
        0xe053 => Code::Delete,
        0xe04f => Code::End,
        0xe047 => Code::Home,
        0xe052 => Code::Insert,
        0xe051 => Code::PageDown,
        0xe049 => Code::PageUp,
        0xe050 => Code::ArrowDown,
        0xe04b => Code::ArrowLeft,
        0xe04d => Code::ArrowRight,
        0xe048 => Code::ArrowUp,
        0xe045 => Code::NumLock,
        0x0052 => Code::Numpad0,
        0x004f => Code::Numpad1,
        0x0050 => Code::Numpad2,
        0x0051 => Code::Numpad3,
        0x004b => Code::Numpad4,
        0x004c => Code::Numpad5,
        0x004d => Code::Numpad6,
        0x0047 => Code::Numpad7,
        0x0048 => Code::Numpad8,
        0x0049 => Code::Numpad9,
        0x004e => Code::NumpadAdd,
        0x007e => Code::NumpadComma,
        0x0053 => Code::NumpadDecimal,
        0xe035 => Code::NumpadDivide,
        0xe01c => Code::NumpadEnter,
        0x0059 => Code::NumpadEqual,
        0x0037 => Code::NumpadMultiply,
        0x004a => Code::NumpadSubtract,
        0x0001 => Code::Escape,
        0x003b => Code::F1,
        0x003c => Code::F2,
        0x003d => Code::F3,
        0x003e => Code::F4,
        0x003f => Code::F5,
        0x0040 => Code::F6,
        0x0041 => Code::F7,
        0x0042 => Code::F8,
        0x0043 => Code::F9,
        0x0044 => Code::F10,
        0x0057 => Code::F11,
        0x0058 => Code::F12,
        0x0064 => Code::F13,
        0x0065 => Code::F14,
        0x0066 => Code::F15,
        0x0067 => Code::F16,
        0x0068 => Code::F17,
        0x0069 => Code::F18,
        0x006a => Code::F19,
        0x006b => Code::F20,
        0x006c => Code::F21,
        0x006d => Code::F22,
        0x006e => Code::F23,
        0x0076 => Code::F24,
        0xe037 => Code::PrintScreen,
        0x0054 => Code::PrintScreen, // Alt + PrintScreen
        0x0046 => Code::ScrollLock,
        0x0045 => Code::Pause,
        0xe046 => Code::Pause, // Ctrl + Pause
        0xe06a => Code::BrowserBack,
        0xe066 => Code::BrowserFavorites,
        0xe069 => Code::BrowserForward,
        0xe032 => Code::BrowserHome,
        0xe067 => Code::BrowserRefresh,
        0xe065 => Code::BrowserSearch,
        0xe068 => Code::BrowserStop,
        0xe06b => Code::LaunchApp1,
        0xe021 => Code::LaunchApp2,
        0xe06c => Code::LaunchMail,
        0xe022 => Code::MediaPlayPause,
        0xe06d => Code::MediaSelect,
        0xe024 => Code::MediaStop,
        0xe019 => Code::MediaTrackNext,
        0xe010 => Code::MediaTrackPrevious,
        0xe05e => Code::Power,
        0xe02e => Code::AudioVolumeDown,
        0xe020 => Code::AudioVolumeMute,
        0xe030 => Code::AudioVolumeUp,

        // Extra from Chromium sources, as in `winit-win32`:
        // https://chromium.googlesource.com/chromium/src.git/+/3e1a26c44c024d97dc9a4c09bbc6a2365398ca2c/ui/events/keycodes/dom/dom_code_data.inc
        0x0077 => Code::Lang4,
        0x0078 => Code::Lang3,
        0xe008 => Code::Undo,
        0xe00a => Code::Paste,
        0xe017 => Code::Cut,
        0xe018 => Code::Copy,
        0xe02c => Code::Eject,
        0xe03b => Code::Help,
        0xe05f => Code::Sleep,
        0xe063 => Code::WakeUp,

        _ => Code::Unidentified,
    }
}

/// Convert a virtual-key code into a [`Location`].
///
/// Inlined from `winit-win32`'s `get_location`, but takes the virtual key
/// directly (from `wParam`) rather than re-deriving it from the scancode via
/// `MapVirtualKeyExW`, since the OS already gave us the virtual key.
fn get_location(vkey: VIRTUAL_KEY, extended: bool) -> Location {
    const ABNT_C2: VIRTUAL_KEY = VK_ABNT_C2;

    // This is taken from the `druid` GUI library, specifically
    // druid-shell/src/platform/windows/keyboard.rs, by way of `winit-win32`.
    match vkey {
        VK_LSHIFT | VK_LCONTROL | VK_LMENU | VK_LWIN => Location::Left,
        VK_RSHIFT | VK_RCONTROL | VK_RMENU | VK_RWIN => Location::Right,
        VK_RETURN if extended => Location::Numpad,
        VK_INSERT | VK_DELETE | VK_END | VK_DOWN | VK_NEXT | VK_LEFT | VK_CLEAR | VK_RIGHT
        | VK_HOME | VK_UP | VK_PRIOR => {
            if extended {
                Location::Standard
            } else {
                Location::Numpad
            }
        }
        VK_NUMPAD0 | VK_NUMPAD1 | VK_NUMPAD2 | VK_NUMPAD3 | VK_NUMPAD4 | VK_NUMPAD5
        | VK_NUMPAD6 | VK_NUMPAD7 | VK_NUMPAD8 | VK_NUMPAD9 | VK_DECIMAL | VK_DIVIDE
        | VK_MULTIPLY | VK_SUBTRACT | VK_ADD | ABNT_C2 => Location::Numpad,
        _ => Location::Standard,
    }
}

/// Convert a virtual-key code to a [`NamedKey`], for non-printable keys.
///
/// Inlined from the non-printable arms of `winit-win32`'s
/// `vkey_to_non_char_key` (in `keyboard_layout.rs`); printable keys (letters,
/// digits, OEM punctuation, etc.) are instead resolved by [`vkey_to_char`]
/// using the active keyboard layout.
fn vkey_to_named_key(vkey: VIRTUAL_KEY) -> Option<NamedKey> {
    use windows_sys::Win32::System::SystemServices::{LANG_JAPANESE, LANG_KOREAN};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

    let hkl = unsafe { GetKeyboardLayout(0) }.addr();
    let primary_lang_id = (((hkl as u32) & 0xffff) as u16) & 0x3ff;
    let is_korean = primary_lang_id as u32 == LANG_KOREAN;
    let is_japanese = primary_lang_id as u32 == LANG_JAPANESE;

    Some(match vkey {
        VK_BACK => NamedKey::Backspace,
        VK_TAB => NamedKey::Tab,
        VK_CLEAR => NamedKey::Clear,
        VK_RETURN => NamedKey::Enter,
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT => NamedKey::Shift,
        VK_CONTROL | VK_LCONTROL | VK_RCONTROL => NamedKey::Control,
        VK_MENU | VK_LMENU | VK_RMENU => NamedKey::Alt,
        // TODO: VK_RMENU => NamedKey::AltGraph,
        VK_PAUSE => NamedKey::Pause,
        VK_CAPITAL => NamedKey::CapsLock,
        VK_HANGUL if is_korean => NamedKey::HangulMode,
        VK_KANA if is_japanese => NamedKey::KanaMode,
        VK_JUNJA => NamedKey::JunjaMode,
        VK_FINAL => NamedKey::FinalMode,
        VK_HANJA if is_korean => NamedKey::HanjaMode,
        VK_KANJI if is_japanese => NamedKey::KanjiMode,
        VK_ESCAPE => NamedKey::Escape,
        VK_CONVERT => NamedKey::Convert,
        VK_NONCONVERT => NamedKey::NonConvert,
        VK_ACCEPT => NamedKey::Accept,
        VK_MODECHANGE => NamedKey::ModeChange,
        VK_PRIOR => NamedKey::PageUp,
        VK_NEXT => NamedKey::PageDown,
        VK_END => NamedKey::End,
        VK_HOME => NamedKey::Home,
        VK_LEFT => NamedKey::ArrowLeft,
        VK_UP => NamedKey::ArrowUp,
        VK_RIGHT => NamedKey::ArrowRight,
        VK_DOWN => NamedKey::ArrowDown,
        VK_SELECT => NamedKey::Select,
        VK_PRINT => NamedKey::Print,
        VK_EXECUTE => NamedKey::Execute,
        VK_SNAPSHOT => NamedKey::PrintScreen,
        VK_INSERT => NamedKey::Insert,
        VK_DELETE => NamedKey::Delete,
        VK_HELP => NamedKey::Help,
        VK_LWIN | VK_RWIN => NamedKey::Meta,
        VK_APPS => NamedKey::ContextMenu,
        VK_SLEEP => NamedKey::Standby,
        VK_F1 => NamedKey::F1,
        VK_F2 => NamedKey::F2,
        VK_F3 => NamedKey::F3,
        VK_F4 => NamedKey::F4,
        VK_F5 => NamedKey::F5,
        VK_F6 => NamedKey::F6,
        VK_F7 => NamedKey::F7,
        VK_F8 => NamedKey::F8,
        VK_F9 => NamedKey::F9,
        VK_F10 => NamedKey::F10,
        VK_F11 => NamedKey::F11,
        VK_F12 => NamedKey::F12,
        VK_F13 => NamedKey::F13,
        VK_F14 => NamedKey::F14,
        VK_F15 => NamedKey::F15,
        VK_F16 => NamedKey::F16,
        VK_F17 => NamedKey::F17,
        VK_F18 => NamedKey::F18,
        VK_F19 => NamedKey::F19,
        VK_F20 => NamedKey::F20,
        VK_F21 => NamedKey::F21,
        VK_F22 => NamedKey::F22,
        VK_F23 => NamedKey::F23,
        VK_F24 => NamedKey::F24,
        VK_NUMLOCK => NamedKey::NumLock,
        VK_SCROLL => NamedKey::ScrollLock,
        VK_BROWSER_BACK => NamedKey::BrowserBack,
        VK_BROWSER_FORWARD => NamedKey::BrowserForward,
        VK_BROWSER_REFRESH => NamedKey::BrowserRefresh,
        VK_BROWSER_STOP => NamedKey::BrowserStop,
        VK_BROWSER_SEARCH => NamedKey::BrowserSearch,
        VK_BROWSER_FAVORITES => NamedKey::BrowserFavorites,
        VK_BROWSER_HOME => NamedKey::BrowserHome,
        VK_VOLUME_MUTE => NamedKey::AudioVolumeMute,
        VK_VOLUME_DOWN => NamedKey::AudioVolumeDown,
        VK_VOLUME_UP => NamedKey::AudioVolumeUp,
        VK_MEDIA_NEXT_TRACK => NamedKey::MediaTrackNext,
        VK_MEDIA_PREV_TRACK => NamedKey::MediaTrackPrevious,
        VK_MEDIA_STOP => NamedKey::MediaStop,
        VK_MEDIA_PLAY_PAUSE => NamedKey::MediaPlayPause,
        VK_LAUNCH_MAIL => NamedKey::LaunchMail,
        VK_LAUNCH_MEDIA_SELECT => NamedKey::LaunchMediaPlayer,
        VK_LAUNCH_APP1 => NamedKey::LaunchApplication1,
        VK_LAUNCH_APP2 => NamedKey::LaunchApplication2,
        _ => return None,
    })
}

/// Query the live state of the modifier keys via `GetKeyState`.
///
/// Inlined from `winit-win32`'s `LayoutCache::get_agnostic_mods`. `AltGr` is
/// reported by Windows as a fake `Ctrl`+`Alt` press; when the right `Alt` key
/// is down we filter that synthetic state out of `Ctrl` and `Alt`, matching
/// upstream `winit`'s behavior.
pub fn current_modifiers() -> Modifiers {
    let key_pressed = |vkey: VIRTUAL_KEY| {
        // SAFETY: `GetKeyState` is a pure query function; any `i32` virtual
        // key value is safe to pass.
        let state = unsafe { GetKeyState(vkey as i32) };
        (state as u16 & 0x8000) != 0
    };

    let filter_out_altgr = key_pressed(VK_RMENU);

    let mut modifiers = Modifiers::default();
    if key_pressed(VK_SHIFT) {
        modifiers.insert(Modifiers::SHIFT);
    }
    if key_pressed(VK_CONTROL) && !filter_out_altgr {
        modifiers.insert(Modifiers::CONTROL);
    }
    if key_pressed(VK_MENU) && !filter_out_altgr {
        modifiers.insert(Modifiers::ALT);
    }
    if key_pressed(VK_LWIN) || key_pressed(VK_RWIN) {
        modifiers.insert(Modifiers::META);
    }
    modifiers
}

/// Resolve the printable character produced by a virtual key, respecting the
/// active keyboard layout and the live Shift/Ctrl/Alt/CapsLock state.
///
/// Inlined from the `ToUnicode`-based character resolution that
/// `winit-win32`'s `Layout::get_key` performs (by way of `keyboard_layout.rs`).
/// Unlike upstream `winit`, this calls `ToUnicode` directly per keystroke
/// rather than caching per-layout results, and it does not special-case the
/// combination of a dead key followed by a second keystroke: it always
/// reports the immediate `ToUnicode` result for the current key alone.
///
/// Returns `None` when the virtual key does not produce a character (for
/// example, a control key, or a Ctrl-combination that Windows refuses to
/// translate).
fn vkey_to_char(vkey: VIRTUAL_KEY, scancode: u32) -> Option<char> {
    let mut key_state = [0u8; 256];
    // SAFETY: `key_state` is a valid, correctly sized buffer for `GetKeyboardState`.
    unsafe { GetKeyboardState(key_state.as_mut_ptr()) };

    let mut buf = [0u16; 4];
    // SAFETY: `key_state` and `buf` are valid buffers of the sizes passed in.
    let result = unsafe {
        ToUnicode(
            vkey as u32,
            scancode,
            key_state.as_ptr(),
            buf.as_mut_ptr(),
            buf.len() as i32,
            0,
        )
    };

    // A negative result means the key is a dead key; the unaccented character
    // it would otherwise produce is still written into `buf`.
    let len = if result < 0 { 1 } else { result as usize };
    if len == 0 || len > buf.len() {
        return None;
    }

    OsString::from_wide(&buf[..len])
        .into_string()
        .ok()
        .and_then(|s| s.chars().next())
}

/// Convert a `WM_KEYDOWN`/`WM_KEYUP`/`WM_SYSKEYDOWN`/`WM_SYSKEYUP` message
/// into a [`KeyboardEvent`].
///
/// This is the Win32 equivalent of `ui-events-winit`'s
/// `from_winit_keyboard_event`. It is inlined from the relevant fragments of
/// `winit-win32`'s `PartialKeyEventInfo::from_message` and
/// `vkey_to_non_char_key`/`Layout::get_key`, simplified to a single-message
/// translation: it does not reproduce the `PeekMessage`-driven
/// `WM_KEYDOWN`/`WM_DEADCHAR`/`WM_CHAR` sequencing winit uses to combine dead
/// keys, and `text_with_all_modifiers`/`key_without_modifiers` are not
/// tracked by [`ui_events::keyboard::KeyboardEvent`], so they are omitted
/// here exactly as they are in `ui-events-winit`.
pub fn from_win32_keyboard_event(wparam: WPARAM, lparam: LPARAM, state: KeyState) -> KeyboardEvent {
    let lparam_struct = destructure_key_lparam(lparam);
    let vkey = wparam as VIRTUAL_KEY;
    let scancode = if lparam_struct.scancode == 0 {
        // In some cases (often with media keys) the device reports a scancode
        // of 0 but a valid virtual key. In these cases we obtain the
        // scancode from the virtual key, as `winit-win32` does.
        // SAFETY: `MapVirtualKeyW` is a pure query function; passing an
        // arbitrary `u32` virtual key is safe.
        unsafe { MapVirtualKeyW(vkey as u32, MAPVK_VK_TO_VSC) as u16 }
    } else {
        new_ex_scancode(lparam_struct.scancode, lparam_struct.extended)
    };

    let code = scancode_to_code(scancode as u32);
    let location = get_location(vkey, lparam_struct.extended);
    let modifiers = current_modifiers();

    let key = match vkey_to_named_key(vkey) {
        Some(named) => Key::Named(named),
        None => match vkey_to_char(vkey, scancode as u32) {
            Some(c) => Key::Character(c.to_string()),
            None => Key::Named(NamedKey::Unidentified),
        },
    };

    KeyboardEvent {
        key,
        code,
        modifiers,
        location,
        is_composing: false,
        repeat: lparam_struct.is_repeat,
        state,
    }
}
