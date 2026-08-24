// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UIKit-facing helpers for querying host-side text-input state.

use objc2::rc::Retained;
use objc2_core_foundation::{CGPoint, CGRect};
use objc2_foundation::{NSRange, NSString};
use ui_events::text::{TextRangeEncoding, TextTargetRange};
use ui_text_input::{
    TextClosestPositionProvider, TextGeometryProvider, TextHitTestProvider, TextInputHost,
    TextInputRect, TextRangeConverter, TextRangeProvider,
};

/// Return whether the host currently exposes any editable text.
pub fn has_text_from_host(host: &(impl TextInputHost + TextRangeConverter + ?Sized)) -> bool {
    document_range_from_host(host).is_some_and(|range| range.length > 0)
}

/// Return the full editable document range in UTF-16 code units.
pub fn document_range_from_host(
    host: &(impl TextInputHost + TextRangeConverter + ?Sized),
) -> Option<NSRange> {
    host.text_input_snapshot()
        .document_range()
        .and_then(|range| target_range_to_nsrange(host, range))
}

/// Return the current selected text range in UTF-16 code units.
pub fn selected_text_range_from_host(
    host: &(impl TextInputHost + TextRangeConverter + ?Sized),
) -> Option<NSRange> {
    host.text_input_snapshot()
        .selection()
        .and_then(|selection| target_range_to_nsrange(host, selection.ordered_range()))
}

/// Return the current marked text range in UTF-16 code units.
pub fn marked_text_range_from_host(
    host: &(impl TextInputHost + TextRangeConverter + ?Sized),
) -> Option<NSRange> {
    host.text_input_snapshot()
        .composition()
        .and_then(|composition| target_range_to_nsrange(host, composition.range()))
}

/// Return the text for the exact UTF-16 document range.
///
/// Unlike AppKit's proposed-range query, UIKit's text-range query does not
/// provide an out parameter for an adjusted range. This therefore returns
/// `None` if the host cannot provide exactly `range`.
pub fn text_in_range_from_host(
    host: &(impl TextRangeProvider + ?Sized),
    range: NSRange,
) -> Option<Retained<NSString>> {
    let requested = nsrange_to_utf16_target_range(range)?;
    let slice = host.text_for_range(requested)?;
    (slice.range() == requested).then_some(())?;
    Some(NSString::from_str(slice.text()))
}

/// Return the first rectangle covering the given UTF-16 document range.
pub fn first_rect_for_range_from_host<F>(
    host: &(impl TextGeometryProvider + ?Sized),
    range: NSRange,
    map_rect: F,
) -> Option<CGRect>
where
    F: FnOnce(TextInputRect) -> CGRect,
{
    let range = nsrange_to_utf16_target_range(range)?;
    Some(map_rect(host.first_rect_for_range(range)?.rect))
}

/// Return the caret rectangle when `position_utf16` matches the current
/// collapsed selection.
pub fn caret_rect_for_current_selection_from_host<F>(
    host: &(impl TextInputHost + TextGeometryProvider + TextRangeConverter + ?Sized),
    position_utf16: u32,
    map_rect: F,
) -> Option<CGRect>
where
    F: FnOnce(TextInputRect) -> CGRect,
{
    let selection = selected_text_range_from_host(host)?;
    let start = u32::try_from(selection.location).ok()?;
    (selection.length == 0 && start == position_utf16).then_some(())?;
    Some(map_rect(host.caret_rect()?))
}

/// Return the UTF-16 offset of the character containing the UIKit point.
///
/// This is the exact query used by character-range hit testing. Points outside
/// every character return `None`.
pub fn character_offset_at_point_from_host<F>(
    host: &(impl TextHitTestProvider + ?Sized),
    point: CGPoint,
    map_point: F,
) -> Option<usize>
where
    F: FnOnce(CGPoint) -> dpi::PhysicalPosition<f64>,
{
    host.text_offset_at_point(map_point(point), TextRangeEncoding::Utf16CodeUnits)
        .and_then(|offset| usize::try_from(offset).ok())
}

/// Return the closest UTF-16 document offset to the UIKit point.
///
/// This is intentionally separate from exact character hit testing because
/// UIKit's `closestPositionToPoint:` must also resolve points outside text.
pub fn closest_offset_to_point_from_host<F>(
    host: &(impl TextClosestPositionProvider + ?Sized),
    point: CGPoint,
    map_point: F,
) -> Option<usize>
where
    F: FnOnce(CGPoint) -> dpi::PhysicalPosition<f64>,
{
    host.closest_text_offset_to_point(map_point(point), TextRangeEncoding::Utf16CodeUnits)
        .and_then(|offset| usize::try_from(offset).ok())
}

fn target_range_to_nsrange(
    host: &(impl TextRangeConverter + ?Sized),
    range: TextTargetRange,
) -> Option<NSRange> {
    let range = if range.encoding == TextRangeEncoding::Utf16CodeUnits {
        range
    } else {
        host.convert_range(range, TextRangeEncoding::Utf16CodeUnits)?
    };
    Some(range_to_nsrange(range))
}

fn nsrange_to_utf16_target_range(range: NSRange) -> Option<TextTargetRange> {
    let start = u32::try_from(range.location).ok()?;
    let end = range.location.checked_add(range.length)?;
    Some(TextTargetRange::utf16_code_units(
        start,
        u32::try_from(end).ok()?,
    ))
}

fn range_to_nsrange(range: TextTargetRange) -> NSRange {
    debug_assert_eq!(
        range.encoding,
        TextRangeEncoding::Utf16CodeUnits,
        "UIKit ranges must be expressed in UTF-16 code units",
    );
    debug_assert!(
        range.range.start <= range.range.end,
        "UIKit ranges must not be reversed",
    );
    let start = usize::try_from(range.range.start).unwrap_or_default();
    let length = range
        .range
        .end
        .checked_sub(range.range.start)
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or_default();
    NSRange::new(start, length)
}

#[cfg(test)]
mod tests {
    use alloc::string::{String, ToString};

    use super::{
        caret_rect_for_current_selection_from_host, character_offset_at_point_from_host,
        closest_offset_to_point_from_host, document_range_from_host, has_text_from_host,
        marked_text_range_from_host, selected_text_range_from_host, text_in_range_from_host,
    };
    use dpi::{PhysicalPosition, PhysicalSize};
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_foundation::NSRange;
    use ui_events::text::{TextRangeEncoding, TextTargetRange};
    use ui_text_input::{
        CompositionSnapshot, TextClosestPositionProvider, TextGeometryProvider,
        TextHitTestProvider, TextInputHost, TextInputRect, TextInputSnapshot, TextRangeConverter,
        TextRangeProvider, TextRangeRect, TextRangeSlice, TextSelection,
    };

    #[derive(Debug, Default)]
    struct StubHost;

    impl TextInputHost for StubHost {
        fn text_input_snapshot(&self) -> TextInputSnapshot {
            TextInputSnapshot::new()
                .with_document_range(TextTargetRange::utf16_code_units(0, 5))
                .with_selection(TextSelection::utf8_bytes(1, 1))
                .with_composition(CompositionSnapshot::new(TextTargetRange::utf16_code_units(
                    1, 3,
                )))
        }
    }

    impl TextRangeConverter for StubHost {
        fn convert_range(
            &self,
            range: TextTargetRange,
            encoding: TextRangeEncoding,
        ) -> Option<TextTargetRange> {
            match (range, encoding) {
                (
                    TextTargetRange {
                        range: ui_events::text::TextRange { start: 1, end: 1 },
                        encoding: TextRangeEncoding::Utf8Bytes,
                    },
                    TextRangeEncoding::Utf16CodeUnits,
                ) => Some(TextTargetRange::utf16_code_units(1, 1)),
                (range, encoding) if range.encoding == encoding => Some(range),
                _ => None,
            }
        }
    }

    impl TextRangeProvider for StubHost {
        fn text_for_range(&self, range: TextTargetRange) -> Option<TextRangeSlice> {
            (range == TextTargetRange::utf16_code_units(1, 3))
                .then(|| TextRangeSlice::new(String::from("🙂"), range).expect("valid exact slice"))
        }
    }

    impl TextGeometryProvider for StubHost {
        fn caret_rect(&self) -> Option<TextInputRect> {
            Some(TextInputRect::new(
                PhysicalPosition::new(10.0, 20.0),
                PhysicalSize::new(1.0, 18.0),
            ))
        }

        fn first_rect_for_range(&self, range: TextTargetRange) -> Option<TextRangeRect> {
            Some(TextRangeRect::new(
                range,
                TextInputRect::new(
                    PhysicalPosition::new(10.0, 20.0),
                    PhysicalSize::new(12.0, 18.0),
                ),
            ))
        }
    }

    impl TextHitTestProvider for StubHost {
        fn text_offset_at_point(
            &self,
            point: PhysicalPosition<f64>,
            encoding: TextRangeEncoding,
        ) -> Option<u32> {
            (point == PhysicalPosition::new(12.0, 24.0)
                && encoding == TextRangeEncoding::Utf16CodeUnits)
                .then_some(2)
        }
    }

    impl TextClosestPositionProvider for StubHost {
        fn closest_text_offset_to_point(
            &self,
            _point: PhysicalPosition<f64>,
            encoding: TextRangeEncoding,
        ) -> Option<u32> {
            (encoding == TextRangeEncoding::Utf16CodeUnits).then_some(5)
        }
    }

    #[test]
    fn helpers_expose_ranges_and_exact_text() {
        let host = StubHost;
        assert!(has_text_from_host(&host));
        assert_eq!(document_range_from_host(&host), Some(NSRange::new(0, 5)));
        assert_eq!(
            selected_text_range_from_host(&host),
            Some(NSRange::new(1, 0))
        );
        assert_eq!(marked_text_range_from_host(&host), Some(NSRange::new(1, 2)));
        let text = text_in_range_from_host(&host, NSRange::new(1, 2)).expect("text");
        assert_eq!(text.to_string(), "🙂");
        assert!(text_in_range_from_host(&host, NSRange::new(0, 5)).is_none());
    }

    #[test]
    fn helpers_keep_exact_and_closest_hit_tests_distinct() {
        let host = StubHost;
        let rect = caret_rect_for_current_selection_from_host(&host, 1, |rect| {
            CGRect::new(
                CGPoint::new(rect.origin.x, rect.origin.y),
                CGSize::new(rect.size.width, rect.size.height),
            )
        })
        .expect("caret rect");
        assert_eq!(
            rect,
            CGRect::new(CGPoint::new(10.0, 20.0), CGSize::new(1.0, 18.0))
        );
        assert_eq!(
            character_offset_at_point_from_host(&host, CGPoint::new(1.0, 2.0), |_point| {
                PhysicalPosition::new(120.0, 240.0)
            }),
            None
        );
        assert_eq!(
            closest_offset_to_point_from_host(&host, CGPoint::new(1.0, 2.0), |_point| {
                PhysicalPosition::new(120.0, 240.0)
            }),
            Some(5)
        );
    }
}
