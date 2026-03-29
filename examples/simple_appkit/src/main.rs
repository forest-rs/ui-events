// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal macOS AppKit example that converts `NSEvent` into `ui-events` via
//! `ui-events-appkit` and draws a simple visualization in a custom `NSView`.

#![allow(unsafe_code, reason = "We access platform libraries using ffi.")]
// Keep example code compact without exhaustive docs.

#[cfg(target_os = "macos")]
mod appkit_example {
    use std::cell::RefCell;

    use objc2::MainThreadMarker;
    use objc2::rc::{Retained, autoreleasepool};
    use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSBezierPath, NSColor, NSEvent, NSResponder,
        NSStringDrawing, NSView, NSWindow, NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use ui_events::keyboard::KeyboardEvent;
    use ui_events::pointer::PointerEvent as UiPointerEvent;

    #[derive(Default, Debug)]
    struct VizState {
        tracker: RefCell<example_input_tracker::InputTracker>,
    }

    define_class!(
        #[unsafe(super = NSView)]
        #[thread_kind = objc2::MainThreadOnly]
        #[ivars = VizState]
        struct VizView;

        impl VizView {
            #[unsafe(method(acceptsFirstResponder))]
            fn accepts_first_responder(&self) -> bool { false }
            #[unsafe(method(isFlipped))]
            fn is_flipped(&self) -> bool { true }

            #[unsafe(method(drawRect:))]
            fn draw_rect(&self, _rect: NSRect) {
                // Background fill
                NSColor::blackColor().setFill();
                let bounds = self.bounds();
                NSBezierPath::fillRect(bounds);

                // Draw like simple_web: dots only (no polyline)
                let samples = self.ivars().tracker.borrow();

                // Coalesced points (light blue)
                for s in samples.coalesced().iter().copied() {
                    let p = point_sample_to_view(self, s);
                    let dot = NSBezierPath::bezierPathWithOvalInRect(NSRect::new(
                        NSPoint::new(p.x - 2.0, p.y - 2.0),
                        NSSize::new(4.0, 4.0),
                    ));
                    NSColor::colorWithSRGBRed_green_blue_alpha(64.0/255.0, 160.0/255.0, 1.0, 0.3).setFill();
                    dot.fill();
                }
                // Predicted points (orange)
                for s in samples.predicted().iter().copied() {
                    let p = point_sample_to_view(self, s);
                    let dot = NSBezierPath::bezierPathWithOvalInRect(NSRect::new(
                        NSPoint::new(p.x - 3.0, p.y - 3.0),
                        NSSize::new(6.0, 6.0),
                    ));
                    NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 160.0/255.0, 0.0, 0.45).setFill();
                    dot.fill();
                }
                // Current trail (solid blue dots with stroke)
                for s in samples.current().iter().copied() {
                    let p = point_sample_to_view(self, s);
                    let dot = NSBezierPath::bezierPathWithOvalInRect(NSRect::new(
                        NSPoint::new(p.x - 3.0, p.y - 3.0),
                        NSSize::new(6.0, 6.0),
                    ));
                    let fill = NSColor::colorWithSRGBRed_green_blue_alpha(64.0/255.0, 160.0/255.0, 1.0, 0.9);
                    fill.setFill();
                    dot.fill();
                    fill.setStroke();
                    dot.setLineWidth(1.0);
                    dot.stroke();
                }

                // Mark downs/ups
                // Down rings (green) and up rings (red), with fill + stroke like web demo
                for s in samples.downs().iter().copied() {
                    let p = point_sample_to_view(self, s);
                    let rect = NSRect::new(NSPoint::new(p.x - 9.0, p.y - 9.0), NSSize::new(18.0, 18.0));
                    let dot = NSBezierPath::bezierPathWithOvalInRect(rect);
                    NSColor::colorWithSRGBRed_green_blue_alpha(0.0, 200.0/255.0, 120.0/255.0, 0.25).setFill();
                    dot.fill();
                    NSColor::colorWithSRGBRed_green_blue_alpha(0.0, 200.0/255.0, 120.0/255.0, 0.9).setStroke();
                    dot.setLineWidth(1.0);
                    dot.stroke();
                }
                for s in samples.ups().iter().copied() {
                    let p = point_sample_to_view(self, s);
                    let rect = NSRect::new(NSPoint::new(p.x - 7.0, p.y - 7.0), NSSize::new(14.0, 14.0));
                    let dot = NSBezierPath::bezierPathWithOvalInRect(rect);
                    NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 80.0/255.0, 80.0/255.0, 0.25).setFill();
                    dot.fill();
                    NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 80.0/255.0, 80.0/255.0, 0.9).setStroke();
                    dot.setLineWidth(1.0);
                    dot.stroke();
                }

                // Scroll indicator vector
                if let Some(scroll) = samples.last_scroll() {
                    let p = logical_point(self, scroll.x / scroll.scale_factor, scroll.y / scroll.scale_factor);
                    let (dx, dy) = match scroll.delta {
                        ui_events::ScrollDelta::PixelDelta(ph) => (
                            ph.x / scroll.scale_factor,
                            -(ph.y / scroll.scale_factor),
                        ),
                        ui_events::ScrollDelta::LineDelta(x, y) => (x as f64 * 12.0, -(y as f64) * 12.0),
                        ui_events::ScrollDelta::PageDelta(x, y) => (x as f64 * 80.0, -(y as f64) * 80.0),
                    };
                    let vec = NSBezierPath::bezierPath();
                    NSColor::systemBlueColor().setStroke();
                    vec.setLineWidth(1.5);
                    vec.moveToPoint(p);
                    vec.lineToPoint(NSPoint::new(p.x + dx, p.y + dy));
                    vec.stroke();
                }

                // HUD background and text
                let hud = hud_clear_rect(self);
                let bg = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(hud, 6.0, 6.0);
                // Light background for contrast with default (black) text
                NSColor::colorWithSRGBRed_green_blue_alpha(0.95, 0.95, 0.95, 0.85).setFill();
                bg.fill();
                NSColor::blackColor().setStroke();
                bg.stroke();

                let keys = {
                    let set = samples.pressed_keys();
                    if set.is_empty() { String::from("(none)") } else { set.iter().cloned().collect::<Vec<_>>().join(", ") }
                };
                let scroll_txt = if let Some(scroll) = samples.last_scroll() {
                    let (dx, dy) = match scroll.delta {
                        ui_events::ScrollDelta::PixelDelta(ph) => (
                            ph.x / scroll.scale_factor,
                            -(ph.y / scroll.scale_factor),
                        ),
                        ui_events::ScrollDelta::LineDelta(x, y) => (x as f64 * 12.0, -(y as f64) * 12.0),
                        ui_events::ScrollDelta::PageDelta(x, y) => (x as f64 * 80.0, -(y as f64) * 80.0),
                    };
                    format!("dx={:.1} dy={:.1}", dx, dy)
                } else { String::from("dx=0 dy=0") };
                let text = NSString::from_str(&format!("Keys: {}    Scroll: {}    [Clear]", keys, scroll_txt));
                unsafe { text.drawAtPoint_withAttributes(NSPoint::new(hud.origin.x + 8.0, hud.origin.y + 5.0), None) };
            }
        }
    );

    struct VizInputHost {
        view: Retained<VizView>,
    }

    impl ui_events_appkit::AppKitInputResponderHost for VizInputHost {
        fn handle_keyboard_event(&self, event: KeyboardEvent) {
            handle_keyboard(&self.view, event);
        }

        fn handle_pointer_event(&self, event: UiPointerEvent) {
            if should_clear_hud_from_pointer_event(&self.view, &event) {
                self.view.ivars().tracker.borrow_mut().clear();
                self.view.setNeedsDisplay(true);
                return;
            }
            handle_pointer(&self.view, event);
        }

        fn pointer_position(&self, event: &NSEvent) -> (f64, f64) {
            let point = self
                .view
                .convertPoint_fromView(event.locationInWindow(), None);
            (point.x, point.y)
        }

        fn pointer_scale_factor(&self) -> f64 {
            self.view
                .window()
                .map(|window| window.backingScaleFactor())
                .unwrap_or(1.0)
        }
    }

    // Minimal NSApplication delegate that terminates after last window closes.
    define_class!(
        #[unsafe(super = NSResponder)]
        #[thread_kind = objc2::MainThreadOnly]
        struct AppDelegate;

        impl AppDelegate {
            #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
            fn application_should_terminate_after_last_window_closed(
                &self,
                _app: &NSApplication,
            ) -> bool {
                true
            }
        }
    );

    fn hud_clear_rect(view: &VizView) -> NSRect {
        let w = view.bounds().size.width;
        let width = (w * 0.7).min(700.0);
        NSRect::new(NSPoint::new(8.0, 8.0), NSSize::new(width, 24.0))
    }

    fn point_sample_to_view(view: &VizView, s: example_input_tracker::PointSample) -> NSPoint {
        logical_point(view, s.x / s.scale_factor, s.y / s.scale_factor)
    }

    fn logical_point(_view: &VizView, x: f64, y: f64) -> NSPoint {
        NSPoint::new(x, y)
    }

    fn should_clear_hud_from_pointer_event(view: &VizView, event: &UiPointerEvent) -> bool {
        let UiPointerEvent::Down(event) = event else {
            return false;
        };
        let point = logical_point(
            view,
            event.state.position.x / event.state.scale_factor,
            event.state.position.y / event.state.scale_factor,
        );
        let clear = hud_clear_rect(view);
        point.x >= clear.origin.x
            && point.x <= clear.origin.x + clear.size.width
            && point.y >= clear.origin.y
            && point.y <= clear.origin.y + clear.size.height
    }

    fn handle_pointer(view: &VizView, ev: UiPointerEvent) {
        view.ivars().tracker.borrow_mut().handle_pointer(&ev);
        view.setNeedsDisplay(true);
    }

    fn handle_keyboard(view: &VizView, ev: KeyboardEvent) {
        view.ivars().tracker.borrow_mut().handle_keyboard(&ev);
        view.setNeedsDisplay(true);
    }

    pub(crate) fn run() {
        autoreleasepool(|_| {
            let mtm = MainThreadMarker::new().expect("must run on main thread");
            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            // Set up delegate to terminate after last window is closed.
            let delegate: Retained<AppDelegate> = {
                let this = AppDelegate::alloc(mtm);
                unsafe { msg_send![this, init] }
            };
            let _: () = unsafe { msg_send![&*app, setDelegate: &*delegate] };

            // Create a window and our custom view
            let content_rect = NSRect::new(NSPoint::new(100., 100.), NSSize::new(800., 600.));
            let style = NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Resizable
                | NSWindowStyleMask::Miniaturizable;
            let window: Retained<NSWindow> = unsafe {
                msg_send![NSWindow::alloc(mtm),
                    initWithContentRect: content_rect,
                    styleMask: style,
                    backing: objc2_app_kit::NSBackingStoreType::Buffered,
                    defer: false]
            };
            // Required when creating NSWindow outside a controller.
            unsafe { window.setReleasedWhenClosed(false) };
            window.center();
            window.setTitle(&NSString::from_str("ui-events AppKit example"));

            let view: Retained<VizView> = {
                let this = VizView::alloc(mtm).set_ivars(VizState::default());
                unsafe { msg_send![super(this), initWithFrame: content_rect] }
            };
            window.setContentView(Some(&view));
            let input_responder = ui_events_appkit::AppKitInputResponder::new(
                mtm,
                Box::new(VizInputHost {
                    view: Retained::clone(&view),
                }),
            );
            // SAFETY: `view` is retained by `window`, `input_responder` is
            // retained in this stack frame for the AppKit run loop lifetime,
            // and the previous next responder is owned by AppKit.
            unsafe {
                let following_responder = view.nextResponder();
                input_responder.set_following_responder(following_responder.as_deref());
                view.setNextResponder(Some(input_responder.as_ref()));
            }
            window.setAcceptsMouseMovedEvents(true);
            let _ = window.makeFirstResponder(Some(input_responder.as_ref()));
            window.makeKeyAndOrderFront(None);
            app.activate();

            // No external HUD widgets; HUD rendered in drawRect and clear handled via click detection.

            // Enter the native AppKit run loop; the app delegate will terminate
            // the app automatically after the last window closes.
            app.run();
        });
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    appkit_example::run();

    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("This example is macOS-only.");
    }
}
