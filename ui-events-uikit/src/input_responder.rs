// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reusable UIKit responder for touch, remote, and keyboard input.

use alloc::{boxed::Box, vec::Vec};

use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_foundation::NSSet;
use objc2_ui_kit::{UIEvent, UIKey, UIPress, UIPressesEvent, UIResponder, UITouch};
use ui_events::keyboard::KeyboardEvent;
use ui_events::pointer::PointerEvent;
use ui_events_apple_common::EventDisposition;

use crate::{
    keyboard_event_from_uikey, keyboard_event_from_uipress, pointer_event_from_touch,
    pointer_event_from_touch_and_event,
};

/// Host-side event sink for [`UIKitInputResponder`].
pub trait UIKitInputResponderHost {
    /// Handle a translated UIKit keyboard or remote-button event and report
    /// whether native responder routing should stop.
    fn handle_keyboard_event(&self, event: KeyboardEvent) -> EventDisposition;

    /// Handle a translated UIKit touch or stylus pointer event and report
    /// whether native responder routing should stop.
    fn handle_pointer_event(&self, event: PointerEvent) -> EventDisposition;

    /// Return the scale factor used when converting UIKit pointer positions
    /// from logical points into physical pixels.
    fn pointer_scale_factor(&self) -> f64;
}

#[doc(hidden)]
pub struct InputResponderState {
    host: Box<dyn UIKitInputResponderHost>,
}

impl core::fmt::Debug for InputResponderState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InputResponderState")
            .field("host", &"<dyn UIKitInputResponderHost>")
            .finish()
    }
}

define_class!(
    #[unsafe(super = UIResponder)]
    #[thread_kind = objc2::MainThreadOnly]
    #[name = "UIEventsUIKitInputResponder"]
    #[ivars = InputResponderState]
    #[doc = "Reusable UIKit responder for touch, remote, and keyboard input."]
    pub struct UIKitInputResponder;

    impl UIKitInputResponder {
        #[unsafe(method(canBecomeFirstResponder))]
        fn can_become_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(touchesBegan:withEvent:))]
        fn touches_began(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            let unhandled = self.unhandled_touches(touches, event);
            if !unhandled.is_empty() {
                let unhandled = NSSet::from_retained_slice(&unhandled);
                // SAFETY: This invokes the overridden method on `UIResponder`.
                unsafe { msg_send![super(self), touchesBegan: &*unhandled, withEvent: event] }
            }
        }

        #[unsafe(method(touchesMoved:withEvent:))]
        fn touches_moved(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            let unhandled = self.unhandled_touches(touches, event);
            if !unhandled.is_empty() {
                let unhandled = NSSet::from_retained_slice(&unhandled);
                // SAFETY: This invokes the overridden method on `UIResponder`.
                unsafe { msg_send![super(self), touchesMoved: &*unhandled, withEvent: event] }
            }
        }

        #[unsafe(method(touchesEnded:withEvent:))]
        fn touches_ended(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            let unhandled = self.unhandled_touches(touches, event);
            if !unhandled.is_empty() {
                let unhandled = NSSet::from_retained_slice(&unhandled);
                // SAFETY: This invokes the overridden method on `UIResponder`.
                unsafe { msg_send![super(self), touchesEnded: &*unhandled, withEvent: event] }
            }
        }

        #[unsafe(method(touchesCancelled:withEvent:))]
        fn touches_cancelled(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            let unhandled = self.unhandled_touches(touches, event);
            if !unhandled.is_empty() {
                let unhandled = NSSet::from_retained_slice(&unhandled);
                // SAFETY: This invokes the overridden method on `UIResponder`.
                unsafe { msg_send![super(self), touchesCancelled: &*unhandled, withEvent: event] }
            }
        }

        #[unsafe(method(pressesBegan:withEvent:))]
        fn presses_began(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            let unhandled = self.unhandled_presses(presses);
            if !unhandled.is_empty() {
                let unhandled = NSSet::from_retained_slice(&unhandled);
                // SAFETY: This invokes the overridden method on `UIResponder`.
                unsafe { msg_send![super(self), pressesBegan: &*unhandled, withEvent: event] }
            }
        }

        #[unsafe(method(pressesChanged:withEvent:))]
        fn presses_changed(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            let unhandled = self.unhandled_presses(presses);
            if !unhandled.is_empty() {
                let unhandled = NSSet::from_retained_slice(&unhandled);
                // SAFETY: This invokes the overridden method on `UIResponder`.
                unsafe { msg_send![super(self), pressesChanged: &*unhandled, withEvent: event] }
            }
        }

        #[unsafe(method(pressesEnded:withEvent:))]
        fn presses_ended(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            let unhandled = self.unhandled_presses(presses);
            if !unhandled.is_empty() {
                let unhandled = NSSet::from_retained_slice(&unhandled);
                // SAFETY: This invokes the overridden method on `UIResponder`.
                unsafe { msg_send![super(self), pressesEnded: &*unhandled, withEvent: event] }
            }
        }

        #[unsafe(method(pressesCancelled:withEvent:))]
        fn presses_cancelled(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            let unhandled = self.unhandled_presses(presses);
            if !unhandled.is_empty() {
                let unhandled = NSSet::from_retained_slice(&unhandled);
                // SAFETY: This invokes the overridden method on `UIResponder`.
                unsafe { msg_send![super(self), pressesCancelled: &*unhandled, withEvent: event] }
            }
        }
    }
);

impl UIKitInputResponder {
    /// Create a reusable UIKit responder for touch, remote, and keyboard input.
    pub fn new(mtm: MainThreadMarker, host: Box<dyn UIKitInputResponderHost>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(InputResponderState { host });
        // SAFETY: `UIResponder` has no additional initialization requirements.
        unsafe { msg_send![super(this), init] }
    }

    /// Translate all touches in a UIKit callback and forward them to the host.
    ///
    /// Returns [`EventDisposition::Handled`] only when the set is nonempty and
    /// the host handles every touch. Responder overrides forward each
    /// unhandled touch without forwarding handled touches again.
    pub fn handle_touch_set(
        &self,
        touches: &NSSet<UITouch>,
        event: Option<&UIEvent>,
    ) -> EventDisposition {
        if !touches.is_empty() && self.unhandled_touches(touches, event).is_empty() {
            EventDisposition::Handled
        } else {
            EventDisposition::Unhandled
        }
    }

    /// Translate a single `UITouch` and optional `UIEvent`, then forward it to
    /// the host.
    pub fn handle_touch(&self, touch: &UITouch, event: Option<&UIEvent>) -> EventDisposition {
        let scale_factor = self.ivars().host.pointer_scale_factor();
        let event = event
            .and_then(|event| pointer_event_from_touch_and_event(touch, event, scale_factor))
            .or_else(|| pointer_event_from_touch(touch, scale_factor));
        if let Some(event) = event {
            self.ivars().host.handle_pointer_event(event)
        } else {
            EventDisposition::Unhandled
        }
    }

    /// Translate all presses in a UIKit callback and forward them to the host.
    ///
    /// Returns [`EventDisposition::Handled`] only when the set is nonempty and
    /// the host handles every press. Responder overrides forward each
    /// unhandled press without forwarding handled presses again.
    pub fn handle_press_set(&self, presses: &NSSet<UIPress>) -> EventDisposition {
        if !presses.is_empty() && self.unhandled_presses(presses).is_empty() {
            EventDisposition::Handled
        } else {
            EventDisposition::Unhandled
        }
    }

    /// Translate a single `UIPress`, preferring attached `UIKey` information
    /// when available, then forward it to the host.
    pub fn handle_press(&self, press: &UIPress, mtm: MainThreadMarker) -> EventDisposition {
        let event = press
            .key(mtm)
            .as_deref()
            .and_then(|key: &UIKey| keyboard_event_from_uikey(press, key))
            .or_else(|| keyboard_event_from_uipress(press));
        if let Some(event) = event {
            self.ivars().host.handle_keyboard_event(event)
        } else {
            EventDisposition::Unhandled
        }
    }

    fn unhandled_touches(
        &self,
        touches: &NSSet<UITouch>,
        event: Option<&UIEvent>,
    ) -> Vec<Retained<UITouch>> {
        touches
            .iter()
            .filter(|touch| !self.handle_touch(touch, event).is_handled())
            .collect()
    }

    fn unhandled_presses(&self, presses: &NSSet<UIPress>) -> Vec<Retained<UIPress>> {
        let mtm = MainThreadMarker::from(self);
        presses
            .iter()
            .filter(|press| !self.handle_press(press, mtm).is_handled())
            .collect()
    }
}
