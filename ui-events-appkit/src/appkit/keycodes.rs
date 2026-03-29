// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! macOS virtual keycode constants used by AppKit's `NSEvent::keyCode()`.
//!
//! These mirror the classic kVK_* values from HIToolbox/Events.h. Keeping them
//! named avoids sprinkling raw numeric literals throughout the mapping tables.

// Letters / punctuation
pub(super) const ANSI_A: u16 = 0;
pub(super) const ANSI_S: u16 = 1;
pub(super) const ANSI_D: u16 = 2;
pub(super) const ANSI_F: u16 = 3;
pub(super) const ANSI_H: u16 = 4;
pub(super) const ANSI_G: u16 = 5;
pub(super) const ANSI_Z: u16 = 6;
pub(super) const ANSI_X: u16 = 7;
pub(super) const ANSI_C: u16 = 8;
pub(super) const ANSI_V: u16 = 9;
pub(super) const ANSI_B: u16 = 11;
pub(super) const ANSI_Q: u16 = 12;
pub(super) const ANSI_W: u16 = 13;
pub(super) const ANSI_E: u16 = 14;
pub(super) const ANSI_R: u16 = 15;
pub(super) const ANSI_Y: u16 = 16;
pub(super) const ANSI_T: u16 = 17;
pub(super) const ANSI_1: u16 = 18;
pub(super) const ANSI_2: u16 = 19;
pub(super) const ANSI_3: u16 = 20;
pub(super) const ANSI_4: u16 = 21;
pub(super) const ANSI_6: u16 = 22;
pub(super) const ANSI_5: u16 = 23;
pub(super) const ANSI_EQUAL: u16 = 24;
pub(super) const ANSI_9: u16 = 25;
pub(super) const ANSI_7: u16 = 26;
pub(super) const ANSI_MINUS: u16 = 27;
pub(super) const ANSI_8: u16 = 28;
pub(super) const ANSI_0: u16 = 29;
pub(super) const ANSI_RIGHT_BRACKET: u16 = 30;
pub(super) const ANSI_O: u16 = 31;
pub(super) const ANSI_U: u16 = 32;
pub(super) const ANSI_LEFT_BRACKET: u16 = 33;
pub(super) const ANSI_I: u16 = 34;
pub(super) const ANSI_P: u16 = 35;
pub(super) const RETURN: u16 = 36;
pub(super) const ANSI_L: u16 = 37;
pub(super) const ANSI_J: u16 = 38;
pub(super) const ANSI_QUOTE: u16 = 39;
pub(super) const ANSI_K: u16 = 40;
pub(super) const ANSI_SEMICOLON: u16 = 41;
pub(super) const ANSI_BACKSLASH: u16 = 42;
pub(super) const ANSI_COMMA: u16 = 43;
pub(super) const ANSI_SLASH: u16 = 44;
pub(super) const ANSI_N: u16 = 45;
pub(super) const ANSI_M: u16 = 46;
pub(super) const ANSI_PERIOD: u16 = 47;
pub(super) const TAB: u16 = 48;
pub(super) const SPACE: u16 = 49;
pub(super) const ANSI_GRAVE: u16 = 50;
pub(super) const DELETE: u16 = 51; // Backspace
pub(super) const ESCAPE: u16 = 53;
pub(super) const RIGHT_COMMAND: u16 = 54; // MetaRight
pub(super) const COMMAND: u16 = 55; // MetaLeft
pub(super) const SHIFT: u16 = 56; // ShiftLeft
pub(super) const CAPS_LOCK: u16 = 57;
pub(super) const OPTION: u16 = 58; // AltLeft
pub(super) const CONTROL: u16 = 59; // ControlLeft
pub(super) const RIGHT_SHIFT: u16 = 60;
pub(super) const RIGHT_OPTION: u16 = 61; // AltRight
pub(super) const RIGHT_CONTROL: u16 = 62; // ControlRight
pub(super) const FUNCTION: u16 = 63; // Fn

// Keypad
pub(super) const ANSI_KEYPAD_DECIMAL: u16 = 65;
pub(super) const ANSI_KEYPAD_MULTIPLY: u16 = 67;
pub(super) const ANSI_KEYPAD_PLUS: u16 = 69;
pub(super) const ANSI_KEYPAD_CLEAR: u16 = 71;
pub(super) const ANSI_KEYPAD_DIVIDE: u16 = 75;
pub(super) const ANSI_KEYPAD_ENTER: u16 = 76;
pub(super) const ANSI_KEYPAD_MINUS: u16 = 78;
pub(super) const ANSI_KEYPAD_EQUALS: u16 = 81;
pub(super) const ANSI_KEYPAD_0: u16 = 82;
pub(super) const ANSI_KEYPAD_1: u16 = 83;
pub(super) const ANSI_KEYPAD_2: u16 = 84;
pub(super) const ANSI_KEYPAD_3: u16 = 85;
pub(super) const ANSI_KEYPAD_4: u16 = 86;
pub(super) const ANSI_KEYPAD_5: u16 = 87;
pub(super) const ANSI_KEYPAD_6: u16 = 88;
pub(super) const ANSI_KEYPAD_7: u16 = 89;
pub(super) const ANSI_KEYPAD_8: u16 = 91;
pub(super) const ANSI_KEYPAD_9: u16 = 92;

// Navigation / function keys
pub(super) const F17: u16 = 64;
pub(super) const F5: u16 = 96;
pub(super) const F6: u16 = 97;
pub(super) const F7: u16 = 98;
pub(super) const F3: u16 = 99;
pub(super) const F8: u16 = 100;
pub(super) const F9: u16 = 101;
pub(super) const F11: u16 = 103;
pub(super) const F13: u16 = 105;
pub(super) const F16: u16 = 106;
pub(super) const F14: u16 = 107;
pub(super) const F10: u16 = 109;
pub(super) const F12: u16 = 111;
pub(super) const F15: u16 = 113;
pub(super) const HELP: u16 = 114;
pub(super) const HOME: u16 = 115;
pub(super) const PAGE_UP: u16 = 116;
pub(super) const FORWARD_DELETE: u16 = 117; // Delete (Forward)
pub(super) const F4: u16 = 118;
pub(super) const END: u16 = 119;
pub(super) const F2: u16 = 120;
pub(super) const PAGE_DOWN: u16 = 121;
pub(super) const F1: u16 = 122;
pub(super) const ARROW_LEFT: u16 = 123;
pub(super) const ARROW_RIGHT: u16 = 124;
pub(super) const ARROW_DOWN: u16 = 125;
pub(super) const ARROW_UP: u16 = 126;
