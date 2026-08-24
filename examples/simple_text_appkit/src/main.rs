// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Minimal native AppKit text editor backed by `AppKitTextInputResponder`.

#![allow(unsafe_code, reason = "We access platform libraries using FFI.")]

#[cfg(target_os = "macos")]
mod appkit_example {
    use std::cell::RefCell;
    use std::ops::Range;

    use dpi::{PhysicalPosition, PhysicalSize};
    use objc2::rc::{Retained, autoreleasepool};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSBezierPath, NSColor, NSResponder,
        NSStringDrawing, NSView, NSWindow, NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
    use ui_events::edit::EditCommandEvent;
    use ui_events::keyboard::KeyboardEvent;
    use ui_events::text::{
        CompositionState, TextInputEvent, TextRange, TextRangeEncoding, TextTargetRange,
    };
    use ui_events_appkit::{AppKitTextInputResponderHost, EventDisposition};
    use ui_text_input::{
        CompositionSnapshot, TextGeometryProvider, TextHitTestProvider, TextInputHost,
        TextInputRect, TextInputSnapshot, TextRangeConverter, TextRangeProvider, TextRangeRect,
        TextRangeSlice, TextSelection, next_utf8_boundary, previous_utf8_boundary,
        utf8_offset_in_encoding, utf8_range_to_target_range,
    };

    const TEXT_X: f64 = 24.0;
    const TEXT_Y: f64 = 92.0;
    const LINE_HEIGHT: f64 = 24.0;

    #[derive(Debug)]
    struct Editor {
        text: String,
        anchor: usize,
        active: usize,
        composition: Option<Range<usize>>,
        revision: u64,
    }

    impl Default for Editor {
        fn default() -> Self {
            let text = String::from("Try Japanese, Chinese, Korean, emoji, or dead keys: ");
            let caret = text.len();
            Self {
                text,
                anchor: caret,
                active: caret,
                composition: None,
                revision: 0,
            }
        }
    }

    impl Editor {
        fn selection_range(&self) -> Range<usize> {
            self.anchor.min(self.active)..self.anchor.max(self.active)
        }

        fn replace(&mut self, range: Range<usize>, replacement: &str) {
            let start = range.start;
            self.text.replace_range(range, replacement);
            let end = start + replacement.len();
            self.anchor = end;
            self.active = end;
            self.revision = self.revision.wrapping_add(1);
        }

        fn range_in_text(&self, range: TextTargetRange) -> Option<Range<usize>> {
            range.to_range_in(&self.text)
        }

        fn apply_text_event(&mut self, event: TextInputEvent) {
            match event {
                TextInputEvent::Insert(insert) => {
                    let explicit = insert
                        .replacement_range
                        .and_then(|range| self.range_in_text(range));
                    let range = explicit
                        .or_else(|| self.composition.take())
                        .unwrap_or_else(|| self.selection_range());
                    self.replace(range, &insert.text);
                    self.composition = None;
                }
                TextInputEvent::CompositionUpdate(composition) => {
                    self.apply_composition(composition);
                }
                TextInputEvent::CompositionEnd => {
                    self.composition = None;
                    self.revision = self.revision.wrapping_add(1);
                }
                TextInputEvent::DeleteBackward => {
                    self.delete_backward();
                }
                TextInputEvent::DeleteForward => {
                    self.delete_forward();
                }
                _ => {}
            }
        }

        fn apply_composition(&mut self, composition: CompositionState) {
            let explicit = composition
                .replacement_range
                .and_then(|range| self.range_in_text(range));
            let range = explicit
                .or_else(|| self.composition.take())
                .unwrap_or_else(|| self.selection_range());
            let start = range.start;
            self.replace(range, &composition.text);
            let end = start + composition.text.len();
            self.composition = Some(start..end);
            if let Some(selection) = composition.selection {
                self.anchor = start + selection.start as usize;
                self.active = start + selection.end as usize;
            }
        }

        fn apply_edit_command(&mut self, command: EditCommandEvent) -> bool {
            match command {
                EditCommandEvent::DeleteBackward
                | EditCommandEvent::DeleteBackwardByDecomposingPreviousCharacter => {
                    self.delete_backward();
                }
                EditCommandEvent::DeleteForward => self.delete_forward(),
                EditCommandEvent::MoveBackward | EditCommandEvent::MoveLeft => {
                    self.move_backward(false);
                }
                EditCommandEvent::MoveForward | EditCommandEvent::MoveRight => {
                    self.move_forward(false);
                }
                EditCommandEvent::MoveBackwardAndModifySelection
                | EditCommandEvent::MoveLeftAndModifySelection => self.move_backward(true),
                EditCommandEvent::MoveForwardAndModifySelection
                | EditCommandEvent::MoveRightAndModifySelection => self.move_forward(true),
                EditCommandEvent::MoveToBeginningOfLine
                | EditCommandEvent::MoveToBeginningOfDocument => self.set_caret(0),
                EditCommandEvent::MoveToEndOfLine | EditCommandEvent::MoveToEndOfDocument => {
                    self.set_caret(self.text.len());
                }
                EditCommandEvent::MoveToBeginningOfLineAndModifySelection
                | EditCommandEvent::MoveToBeginningOfDocumentAndModifySelection => {
                    self.active = 0;
                    self.finish_navigation();
                }
                EditCommandEvent::MoveToEndOfLineAndModifySelection
                | EditCommandEvent::MoveToEndOfDocumentAndModifySelection => {
                    self.active = self.text.len();
                    self.finish_navigation();
                }
                EditCommandEvent::SelectAll => {
                    self.anchor = 0;
                    self.active = self.text.len();
                    self.finish_navigation();
                }
                EditCommandEvent::InsertNewline
                | EditCommandEvent::InsertLineBreak
                | EditCommandEvent::InsertParagraphSeparator => self.insert_literal("\n"),
                EditCommandEvent::InsertTab => self.insert_literal("\t"),
                EditCommandEvent::InsertBacktab => self.insert_literal("    "),
                EditCommandEvent::InsertDoubleQuoteIgnoringSubstitution => {
                    self.insert_literal("\"");
                }
                EditCommandEvent::InsertSingleQuoteIgnoringSubstitution => {
                    self.insert_literal("'");
                }
                EditCommandEvent::CancelOperation if self.composition.is_some() => {
                    self.composition = None;
                    self.revision = self.revision.wrapping_add(1);
                }
                _ => return false,
            }
            true
        }

        fn insert_literal(&mut self, text: &str) {
            let range = self
                .composition
                .take()
                .unwrap_or_else(|| self.selection_range());
            self.replace(range, text);
        }

        fn delete_backward(&mut self) {
            let selection = self.selection_range();
            let range = if selection.is_empty() {
                let Some(start) = previous_utf8_boundary(
                    &self.text,
                    u32::try_from(selection.start).unwrap_or(u32::MAX),
                ) else {
                    return;
                };
                start as usize..selection.end
            } else {
                selection
            };
            self.replace(range, "");
            self.composition = None;
        }

        fn delete_forward(&mut self) {
            let selection = self.selection_range();
            let range = if selection.is_empty() {
                let Some(end) = next_utf8_boundary(
                    &self.text,
                    u32::try_from(selection.end).unwrap_or(u32::MAX),
                ) else {
                    return;
                };
                selection.start..end as usize
            } else {
                selection
            };
            self.replace(range, "");
            self.composition = None;
        }

        fn move_backward(&mut self, extend: bool) {
            let selection = self.selection_range();
            let next = if !extend && !selection.is_empty() {
                selection.start
            } else {
                previous_utf8_boundary(&self.text, u32::try_from(self.active).unwrap_or(u32::MAX))
                    .map_or(self.active, |offset| offset as usize)
            };
            self.active = next;
            if !extend {
                self.anchor = next;
            }
            self.finish_navigation();
        }

        fn move_forward(&mut self, extend: bool) {
            let selection = self.selection_range();
            let next = if !extend && !selection.is_empty() {
                selection.end
            } else {
                next_utf8_boundary(&self.text, u32::try_from(self.active).unwrap_or(u32::MAX))
                    .map_or(self.active, |offset| offset as usize)
            };
            self.active = next;
            if !extend {
                self.anchor = next;
            }
            self.finish_navigation();
        }

        fn set_caret(&mut self, offset: usize) {
            self.anchor = offset;
            self.active = offset;
            self.finish_navigation();
        }

        fn finish_navigation(&mut self) {
            self.composition = None;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    #[derive(Debug, Default)]
    struct TextViewState {
        editor: RefCell<Editor>,
    }

    define_class!(
        #[unsafe(super = NSView)]
        #[thread_kind = objc2::MainThreadOnly]
        #[ivars = TextViewState]
        struct TextView;

        impl TextView {
            #[unsafe(method(isFlipped))]
            fn is_flipped(&self) -> bool {
                true
            }

            #[unsafe(method(drawRect:))]
            fn draw_rect(&self, _dirty_rect: NSRect) {
                NSColor::whiteColor().setFill();
                NSBezierPath::fillRect(self.bounds());

                let instructions = NSString::from_str(
                    "This is a custom Rust editor using NSTextInputClient. Click the input menu or press Control-Space to change IME.",
                );
                // SAFETY: AppKit's drawing context is active during `drawRect:`.
                unsafe {
                    instructions.drawAtPoint_withAttributes(NSPoint::new(24.0, 28.0), None);
                }

                let editor = self.ivars().editor.borrow();
                let text = NSString::from_str(&editor.text);
                // SAFETY: AppKit's drawing context is active during `drawRect:`.
                unsafe {
                    text.drawAtPoint_withAttributes(NSPoint::new(TEXT_X, TEXT_Y), None);
                }

                let status = match &editor.composition {
                    Some(range) => format!(
                        "marked UTF-8 range {}..{} · selection {}→{} · revision {}",
                        range.start, range.end, editor.anchor, editor.active, editor.revision
                    ),
                    None => format!(
                        "no marked text · selection {}→{} · revision {}",
                        editor.anchor, editor.active, editor.revision
                    ),
                };
                let status = NSString::from_str(&status);
                // SAFETY: AppKit's drawing context is active during `drawRect:`.
                unsafe {
                    status.drawAtPoint_withAttributes(NSPoint::new(24.0, 170.0), None);
                }

                let caret = logical_caret_rect(&editor, editor.active);
                NSColor::keyboardFocusIndicatorColor().setFill();
                NSBezierPath::fillRect(caret);

                if let Some(composition) = &editor.composition {
                    let marked = logical_range_rect(&editor, composition.clone());
                    let underline = NSRect::new(
                        NSPoint::new(marked.origin.x, marked.origin.y + LINE_HEIGHT - 3.0),
                        NSSize::new(marked.size.width.max(1.0), 2.0),
                    );
                    NSBezierPath::fillRect(underline);
                }
            }
        }
    );

    struct TextHost {
        view: Retained<TextView>,
    }

    impl TextInputHost for TextHost {
        fn text_input_snapshot(&self) -> TextInputSnapshot {
            let editor = self.view.ivars().editor.borrow();
            let mut snapshot = TextInputSnapshot::new()
                .with_revision(editor.revision)
                .with_document_range(TextTargetRange::utf8_bytes(
                    0,
                    u32::try_from(editor.text.len()).unwrap_or(u32::MAX),
                ))
                .with_selection(TextSelection::utf8_bytes(
                    u32::try_from(editor.anchor).unwrap_or(u32::MAX),
                    u32::try_from(editor.active).unwrap_or(u32::MAX),
                ));
            if let Some(range) = &editor.composition {
                let target = TextTargetRange::utf8_bytes(
                    u32::try_from(range.start).unwrap_or(u32::MAX),
                    u32::try_from(range.end).unwrap_or(u32::MAX),
                );
                let composition = CompositionSnapshot::new(target)
                    .try_with_text(&editor.text[range.clone()])
                    .expect("the editor keeps composition ranges valid");
                snapshot = snapshot.with_composition(composition);
            }
            snapshot
        }
    }

    impl TextRangeProvider for TextHost {
        fn text_for_range(&self, proposed_range: TextTargetRange) -> Option<TextRangeSlice> {
            let editor = self.view.ivars().editor.borrow();
            let range = proposed_range.to_range_in(&editor.text)?;
            TextRangeSlice::new(&editor.text[range], proposed_range)
        }
    }

    impl TextRangeConverter for TextHost {
        fn convert_range(
            &self,
            range: TextTargetRange,
            encoding: TextRangeEncoding,
        ) -> Option<TextTargetRange> {
            let editor = self.view.ivars().editor.borrow();
            let utf8 = range.to_utf8_range_in(&editor.text)?;
            utf8_range_to_target_range(&editor.text, utf8, encoding)
        }
    }

    impl TextGeometryProvider for TextHost {
        fn caret_rect(&self) -> Option<TextInputRect> {
            let editor = self.view.ivars().editor.borrow();
            let rect = logical_caret_rect(&editor, editor.active);
            Some(logical_rect_to_text_rect(&self.view, rect))
        }

        fn first_rect_for_range(&self, range: TextTargetRange) -> Option<TextRangeRect> {
            let editor = self.view.ivars().editor.borrow();
            let utf8 = range.to_utf8_range_in(&editor.text)?;
            let start = utf8.start as usize;
            let requested_end = utf8.end as usize;
            let first_line_end = editor.text[start..requested_end]
                .find('\n')
                .map_or(requested_end, |offset| start + offset);
            let actual_utf8 = TextRange::new(
                u32::try_from(start).ok()?,
                u32::try_from(first_line_end).ok()?,
            );
            let actual = utf8_range_to_target_range(&editor.text, actual_utf8, range.encoding)?;
            let rect = logical_range_rect(&editor, start..first_line_end);
            Some(TextRangeRect::new(
                actual,
                logical_rect_to_text_rect(&self.view, rect),
            ))
        }
    }

    impl TextHitTestProvider for TextHost {
        fn text_offset_at_point(
            &self,
            point: PhysicalPosition<f64>,
            encoding: TextRangeEncoding,
        ) -> Option<u32> {
            let editor = self.view.ivars().editor.borrow();
            let scale = view_scale_factor(&self.view);
            let x = point.x / scale - TEXT_X;
            let y = point.y / scale - TEXT_Y;
            if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
                return None;
            }
            #[expect(
                clippy::cast_possible_truncation,
                reason = "finite nonnegative points are intentionally quantized to text cells"
            )]
            let line_index = (y / LINE_HEIGHT).floor() as usize;
            let line = editor.text.split('\n').nth(line_index)?;
            let mut line_x = 0.0;
            let mut char_offset = None;
            for (offset, character) in line.char_indices() {
                let mut buffer = [0; 4];
                let width = text_width(character.encode_utf8(&mut buffer));
                if x < line_x + width {
                    char_offset = Some(offset);
                    break;
                }
                line_x += width;
            }
            let char_offset = char_offset?;
            let line_start = editor
                .text
                .split_inclusive('\n')
                .take(line_index)
                .map(str::len)
                .sum::<usize>();
            let utf8_offset = u32::try_from(line_start.checked_add(char_offset)?).ok()?;
            utf8_offset_in_encoding(&editor.text, utf8_offset, encoding)
        }
    }

    impl AppKitTextInputResponderHost for TextHost {
        fn handle_keyboard_event(&self, _event: KeyboardEvent) -> EventDisposition {
            EventDisposition::Unhandled
        }

        fn handle_text_input_event(&self, event: TextInputEvent) {
            self.view
                .ivars()
                .editor
                .borrow_mut()
                .apply_text_event(event);
            self.view.setNeedsDisplay(true);
        }

        fn handle_edit_command_event(&self, command: EditCommandEvent) -> EventDisposition {
            let handled = self
                .view
                .ivars()
                .editor
                .borrow_mut()
                .apply_edit_command(command);
            if handled {
                self.view.setNeedsDisplay(true);
                EventDisposition::Handled
            } else {
                EventDisposition::Unhandled
            }
        }

        fn text_input_rect_to_screen_rect(&self, rect: TextInputRect) -> NSRect {
            let scale = view_scale_factor(&self.view);
            let local = NSRect::new(
                NSPoint::new(rect.origin.x / scale, rect.origin.y / scale),
                NSSize::new(rect.size.width / scale, rect.size.height / scale),
            );
            let window_rect = self.view.convertRect_toView(local, None);
            self.view.window().map_or(window_rect, |window| {
                window.convertRectToScreen(window_rect)
            })
        }

        fn screen_point_to_host(&self, point: NSPoint) -> PhysicalPosition<f64> {
            let window_point = self
                .view
                .window()
                .map_or(point, |window| window.convertPointFromScreen(point));
            let local = self.view.convertPoint_fromView(window_point, None);
            let scale = view_scale_factor(&self.view);
            PhysicalPosition::new(local.x * scale, local.y * scale)
        }
    }

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

    fn logical_caret_rect(editor: &Editor, offset: usize) -> NSRect {
        let (line, x) = line_and_x(&editor.text, offset);
        NSRect::new(
            NSPoint::new(TEXT_X + x, TEXT_Y + line as f64 * LINE_HEIGHT),
            NSSize::new(2.0, LINE_HEIGHT),
        )
    }

    fn logical_range_rect(editor: &Editor, range: Range<usize>) -> NSRect {
        let start = logical_caret_rect(editor, range.start);
        let first_line_end = editor.text[range.clone()]
            .find('\n')
            .map_or(range.end, |offset| range.start + offset);
        let width = text_width(&editor.text[range.start..first_line_end]);
        NSRect::new(start.origin, NSSize::new(width.max(1.0), LINE_HEIGHT))
    }

    fn line_and_x(text: &str, offset: usize) -> (usize, f64) {
        let prefix = text.get(..offset).unwrap_or(text);
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count();
        let x = prefix.rsplit('\n').next().map_or(0.0, text_width);
        (line, x)
    }

    fn text_width(text: &str) -> f64 {
        let text = NSString::from_str(text);
        // SAFETY: Passing no attributes cannot violate the method's generic
        // dictionary-value contract.
        unsafe { text.sizeWithAttributes(None).width }
    }

    fn logical_rect_to_text_rect(view: &TextView, rect: NSRect) -> TextInputRect {
        let scale = view_scale_factor(view);
        TextInputRect::new(
            PhysicalPosition::new(rect.origin.x * scale, rect.origin.y * scale),
            PhysicalSize::new(rect.size.width * scale, rect.size.height * scale),
        )
    }

    fn view_scale_factor(view: &TextView) -> f64 {
        view.window()
            .map(|window| window.backingScaleFactor())
            .unwrap_or(1.0)
    }

    pub(crate) fn run() {
        autoreleasepool(|_| {
            let mtm = MainThreadMarker::new().expect("must run on the main thread");
            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

            let delegate: Retained<AppDelegate> = {
                let this = AppDelegate::alloc(mtm);
                // SAFETY: `NSResponder` has no additional initialization requirements.
                unsafe { msg_send![this, init] }
            };
            // SAFETY: The delegate is retained for the duration of `app.run()`.
            let _: () = unsafe { msg_send![&*app, setDelegate: &*delegate] };

            let content_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(860.0, 240.0));
            let style = NSWindowStyleMask::Titled
                | NSWindowStyleMask::Closable
                | NSWindowStyleMask::Resizable
                | NSWindowStyleMask::Miniaturizable;
            // SAFETY: The arguments satisfy `NSWindow`'s designated initializer.
            let window: Retained<NSWindow> = unsafe {
                msg_send![NSWindow::alloc(mtm),
                    initWithContentRect: content_rect,
                    styleMask: style,
                    backing: objc2_app_kit::NSBackingStoreType::Buffered,
                    defer: false]
            };
            // SAFETY: The window is retained in this stack frame through the run loop.
            unsafe { window.setReleasedWhenClosed(false) };
            window.center();
            window.setTitle(&NSString::from_str("ui-events AppKit IME example"));

            let view: Retained<TextView> = {
                let this = TextView::alloc(mtm).set_ivars(TextViewState::default());
                // SAFETY: `content_rect` is a valid view frame.
                unsafe { msg_send![super(this), initWithFrame: content_rect] }
            };
            window.setContentView(Some(&view));

            let responder = ui_events_appkit::AppKitTextInputResponder::new(
                mtm,
                Box::new(TextHost {
                    view: Retained::clone(&view),
                }),
            );
            // SAFETY: `window` retains `view`; `responder` lives through the run
            // loop; and AppKit owns the existing following responder.
            unsafe {
                responder.set_following_responder(view.nextResponder().as_deref());
                view.setNextResponder(Some(responder.as_ref()));
            }
            let _ = window.makeFirstResponder(Some(responder.as_ref()));
            window.makeKeyAndOrderFront(None);
            app.activate();
            app.run();
        });
    }

    #[cfg(test)]
    mod tests {
        use super::Editor;
        use ui_events::edit::EditCommandEvent;
        use ui_events::text::{CompositionState, TextInputEvent, TextRange};

        fn editor_with_caret(text: &str, caret: usize) -> Editor {
            Editor {
                text: text.into(),
                anchor: caret,
                active: caret,
                composition: None,
                revision: 0,
            }
        }

        #[test]
        fn composition_snapshots_replace_the_previous_marked_text() {
            let mut editor = editor_with_caret("ab", 1);
            editor.apply_text_event(TextInputEvent::CompositionUpdate(
                CompositionState::new("🙂").with_selection(TextRange::new(4, 4)),
            ));
            assert_eq!(editor.text, "a🙂b");
            assert_eq!(editor.composition, Some(1..5));
            assert_eq!((editor.anchor, editor.active), (5, 5));

            editor.apply_text_event(TextInputEvent::composition("日本"));
            assert_eq!(editor.text, "a日本b");
            assert_eq!(editor.composition, Some(1..7));
        }

        #[test]
        fn committed_text_replaces_the_active_composition() {
            let mut editor = editor_with_caret("ab", 1);
            editor.apply_text_event(TextInputEvent::composition("に"));
            editor.apply_text_event(TextInputEvent::insert("日"));
            assert_eq!(editor.text, "a日b");
            assert_eq!(editor.composition, None);
            assert_eq!((editor.anchor, editor.active), (4, 4));
        }

        #[test]
        fn navigation_and_deletion_respect_utf8_boundaries() {
            let mut editor = editor_with_caret("a🙂b", 5);
            assert!(editor.apply_edit_command(EditCommandEvent::MoveBackward));
            assert_eq!((editor.anchor, editor.active), (1, 1));
            assert!(editor.apply_edit_command(EditCommandEvent::DeleteForward));
            assert_eq!(editor.text, "ab");
            assert_eq!((editor.anchor, editor.active), (1, 1));
        }
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    appkit_example::run();

    #[cfg(not(target_os = "macos"))]
    eprintln!("This example is macOS-only.");
}
