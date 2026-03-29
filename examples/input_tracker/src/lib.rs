// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared input tracker for examples.
//!
//! Collects pointer and keyboard input into simple deques of points and a set of
//! pressed keys, similar to the visualization used by `examples/simple_web`.
//!
//! This is intentionally tiny and UI-agnostic: it stores raw data (positions in
//! physical pixels with a Y-down axis, and raw scroll deltas) for the examples
//! to render however they like.

#![expect(
    missing_docs,
    reason = "Small example helper: keep code compact without exhaustive docs"
)]

use std::collections::{BTreeSet, VecDeque};

use ui_events::keyboard::KeyboardEvent;
use ui_events::pointer::PointerEvent;

/// Scroll sample captured from a pointer scroll event.
#[derive(Clone, Debug)]
pub struct ScrollSample {
    pub x: f64,
    pub y: f64,
    pub delta: ui_events::ScrollDelta,
    pub scale_factor: f64,
}

/// Position sample captured from pointer state.
#[derive(Clone, Copy, Debug)]
pub struct PointSample {
    pub x: f64,
    pub y: f64,
    pub scale_factor: f64,
}

/// Tracks recent pointer points and pressed keys for examples.
#[derive(Clone, Debug)]
pub struct InputTracker {
    pressed: BTreeSet<String>,
    current: VecDeque<PointSample>,
    coalesced: VecDeque<PointSample>,
    predicted: VecDeque<PointSample>,
    downs: VecDeque<PointSample>,
    ups: VecDeque<PointSample>,
    last_scroll: Option<ScrollSample>,
}

impl Default for InputTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl InputTracker {
    pub const CAP: usize = 4096;

    pub fn new() -> Self {
        Self {
            pressed: BTreeSet::new(),
            current: VecDeque::with_capacity(Self::CAP),
            coalesced: VecDeque::with_capacity(Self::CAP),
            predicted: VecDeque::with_capacity(Self::CAP),
            downs: VecDeque::with_capacity(256),
            ups: VecDeque::with_capacity(256),
            last_scroll: None,
        }
    }

    fn push_cap<T>(buf: &mut VecDeque<T>, val: T, cap: usize) {
        if buf.len() >= cap {
            buf.pop_front();
        }
        buf.push_back(val);
    }

    pub fn clear(&mut self) {
        self.current.clear();
        self.coalesced.clear();
        self.predicted.clear();
        self.downs.clear();
        self.ups.clear();
        self.last_scroll = None;
        self.pressed.clear();
    }

    pub fn handle_pointer(&mut self, pe: &PointerEvent) {
        use ui_events::pointer::PointerEvent as PE;
        match pe {
            PE::Move(update) => {
                for s in &update.coalesced {
                    Self::push_cap(
                        &mut self.coalesced,
                        PointSample {
                            x: s.position.x,
                            y: s.position.y,
                            scale_factor: s.scale_factor,
                        },
                        Self::CAP,
                    );
                }
                for s in &update.predicted {
                    Self::push_cap(
                        &mut self.predicted,
                        PointSample {
                            x: s.position.x,
                            y: s.position.y,
                            scale_factor: s.scale_factor,
                        },
                        Self::CAP,
                    );
                }
                let s = &update.current;
                Self::push_cap(
                    &mut self.current,
                    PointSample {
                        x: s.position.x,
                        y: s.position.y,
                        scale_factor: s.scale_factor,
                    },
                    Self::CAP,
                );
            }
            PE::Down(btn) => {
                let s = &btn.state;
                Self::push_cap(
                    &mut self.downs,
                    PointSample {
                        x: s.position.x,
                        y: s.position.y,
                        scale_factor: s.scale_factor,
                    },
                    512,
                );
            }
            PE::Up(btn) => {
                let s = &btn.state;
                Self::push_cap(
                    &mut self.ups,
                    PointSample {
                        x: s.position.x,
                        y: s.position.y,
                        scale_factor: s.scale_factor,
                    },
                    512,
                );
            }
            PE::Scroll(s) => {
                self.last_scroll = Some(ScrollSample {
                    x: s.state.position.x,
                    y: s.state.position.y,
                    delta: s.delta,
                    scale_factor: s.state.scale_factor,
                });
            }
            _ => {}
        }
    }

    pub fn handle_keyboard(&mut self, ke: &KeyboardEvent) {
        // Prefer Code for recognizable names; fall back to key label.
        let label = if !matches!(ke.code, ui_events::keyboard::Code::Unidentified) {
            format!("{:?}", ke.code)
        } else {
            match &ke.key {
                ui_events::keyboard::Key::Character(s) => s.clone(),
                ui_events::keyboard::Key::Named(n) => format!("{:?}", n),
            }
        };
        match ke.state {
            ui_events::keyboard::KeyState::Down => {
                self.pressed.insert(label);
            }
            ui_events::keyboard::KeyState::Up => {
                self.pressed.remove(&label);
            }
        }
    }

    // Accessors for rendering
    pub fn pressed_keys(&self) -> &BTreeSet<String> {
        &self.pressed
    }
    pub fn current(&self) -> &VecDeque<PointSample> {
        &self.current
    }
    pub fn coalesced(&self) -> &VecDeque<PointSample> {
        &self.coalesced
    }
    pub fn predicted(&self) -> &VecDeque<PointSample> {
        &self.predicted
    }
    pub fn downs(&self) -> &VecDeque<PointSample> {
        &self.downs
    }
    pub fn ups(&self) -> &VecDeque<PointSample> {
        &self.ups
    }
    pub fn last_scroll(&self) -> Option<&ScrollSample> {
        self.last_scroll.as_ref()
    }
}
