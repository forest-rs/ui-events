// Copyright 2026 the UI Events Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! AppKit-facing helpers for querying host-side text-input state.

use objc2::rc::Retained;
use objc2_foundation::{NSAttributedString, NSPoint, NSRange, NSRect, NSString};
use ui_events::text::{TextRangeEncoding, TextTargetRange};
use ui_text_input::{
    TextGeometryProvider, TextHitTestProvider, TextInputHost, TextInputRect, TextRangeConverter,
    TextRangeProvider,
};

/// Return whether the host currently exposes marked text.
pub fn has_marked_text_from_host(host: &(impl TextInputHost + ?Sized)) -> bool {
    host.text_input_snapshot().composition().is_some()
}

/// Return the marked-text document range in AppKit's UTF-16 `NSRange` form.
pub fn marked_range_from_host(
    host: &(impl TextInputHost + TextRangeConverter + ?Sized),
) -> NSRange {
    host.text_input_snapshot()
        .composition()
        .and_then(|composition| target_range_to_appkit_range(host, composition.range()))
        .unwrap_or_else(not_found_range)
}

/// Return the current selection range in AppKit's UTF-16 `NSRange` form.
pub fn selected_range_from_host(
    host: &(impl TextInputHost + TextRangeConverter + ?Sized),
) -> NSRange {
    host.text_input_snapshot()
        .selection()
        .and_then(|selection| target_range_to_appkit_range(host, selection.ordered_range()))
        .unwrap_or_else(not_found_range)
}

/// Return an attributed substring for AppKit
/// `attributedSubstringForProposedRange:actualRange:`.
///
/// When the host supplies an adjusted subset of `proposed_range`, this writes
/// that exact range to `actual_range`. Passing `None` omits the out parameter.
pub fn attributed_substring_for_proposed_range_from_host(
    host: &(impl TextRangeProvider + TextRangeConverter + ?Sized),
    proposed_range: NSRange,
    actual_range: Option<&mut NSRange>,
) -> Option<Retained<NSAttributedString>> {
    let proposed_range = nsrange_to_utf16_target_range(proposed_range)?;
    let slice = host.text_for_range(proposed_range)?;
    let actual = target_range_to_appkit_target_range(host, slice.range())?;
    range_is_subset_of(actual, proposed_range)?;
    if let Some(actual_range) = actual_range {
        *actual_range = range_to_nsrange(actual);
    }
    Some(NSAttributedString::from_nsstring(&NSString::from_str(
        slice.text(),
    )))
}

/// Return the first rectangle covering part of the given AppKit UTF-16 range.
///
/// The host reports the exact subrange covered by its rectangle. This writes
/// that subrange to `actual_range`, when supplied. `map_rect` performs the
/// caller's view-to-screen coordinate conversion.
pub fn first_rect_for_character_range_from_host<F>(
    host: &(impl TextGeometryProvider + TextRangeConverter + ?Sized),
    proposed_range: NSRange,
    actual_range: Option<&mut NSRange>,
    map_rect: F,
) -> Option<NSRect>
where
    F: FnOnce(TextInputRect) -> NSRect,
{
    let proposed_range = nsrange_to_utf16_target_range(proposed_range)?;
    let range_rect = host.first_rect_for_range(proposed_range)?;
    let actual = target_range_to_appkit_target_range(host, range_rect.range)?;
    range_is_subset_of(actual, proposed_range)?;
    if let Some(actual_range) = actual_range {
        *actual_range = range_to_nsrange(actual);
    }
    Some(map_rect(range_rect.rect))
}

/// Return the UTF-16 offset of the character containing the AppKit point.
///
/// `map_point` converts the AppKit point into the host coordinate space used by
/// [`TextHitTestProvider`]. Points outside every character return AppKit's
/// `NSNotFound` value.
pub fn character_index_for_point_from_host<F>(
    host: &(impl TextHitTestProvider + ?Sized),
    point: NSPoint,
    map_point: F,
) -> usize
where
    F: FnOnce(NSPoint) -> dpi::PhysicalPosition<f64>,
{
    host.text_offset_at_point(map_point(point), TextRangeEncoding::Utf16CodeUnits)
        .and_then(|offset| usize::try_from(offset).ok())
        .unwrap_or_else(ns_not_found)
}

pub(crate) fn target_range_to_appkit_range(
    host: &(impl TextRangeConverter + ?Sized),
    range: TextTargetRange,
) -> Option<NSRange> {
    target_range_to_appkit_target_range(host, range).map(range_to_nsrange)
}

fn target_range_to_appkit_target_range(
    host: &(impl TextRangeConverter + ?Sized),
    range: TextTargetRange,
) -> Option<TextTargetRange> {
    if range.encoding == TextRangeEncoding::Utf16CodeUnits {
        Some(range)
    } else {
        host.convert_range(range, TextRangeEncoding::Utf16CodeUnits)
    }
}

pub(crate) fn nsrange_to_utf16_target_range(range: NSRange) -> Option<TextTargetRange> {
    (range.location != ns_not_found()).then_some(())?;
    let start = u32::try_from(range.location).ok()?;
    let end = range.location.checked_add(range.length)?;
    Some(TextTargetRange::utf16_code_units(
        start,
        u32::try_from(end).ok()?,
    ))
}

pub(crate) fn range_to_nsrange(range: TextTargetRange) -> NSRange {
    debug_assert_eq!(
        range.encoding,
        TextRangeEncoding::Utf16CodeUnits,
        "AppKit ranges must be expressed in UTF-16 code units",
    );
    debug_assert!(
        range.range.start <= range.range.end,
        "AppKit ranges must not be reversed",
    );
    let start = usize::try_from(range.range.start).unwrap_or_else(|_| ns_not_found());
    let length = range
        .range
        .end
        .checked_sub(range.range.start)
        .and_then(|length| usize::try_from(length).ok())
        .unwrap_or_default();
    NSRange::new(start, length)
}

pub(crate) fn not_found_range() -> NSRange {
    NSRange::new(ns_not_found(), 0)
}

fn range_is_subset_of(range: TextTargetRange, proposed: TextTargetRange) -> Option<()> {
    (range.encoding == proposed.encoding
        && proposed.range.start <= range.range.start
        && range.range.end <= proposed.range.end)
        .then_some(())
}

pub(crate) const fn ns_not_found() -> usize {
    usize::MAX
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::{
        attributed_substring_for_proposed_range_from_host, character_index_for_point_from_host,
        first_rect_for_character_range_from_host, has_marked_text_from_host,
        marked_range_from_host, selected_range_from_host,
    };
    use dpi::{PhysicalPosition, PhysicalSize};
    use objc2_foundation::{NSPoint, NSRange, NSRect, NSSize};
    use ui_events::text::{TextRangeEncoding, TextTargetRange};
    use ui_text_input::{
        CompositionSnapshot, TextGeometryProvider, TextHitTestProvider, TextInputHost,
        TextInputRect, TextInputSnapshot, TextRangeConverter, TextRangeProvider, TextRangeRect,
        TextRangeSlice, TextSelection,
    };

    #[derive(Debug, Default)]
    struct StubHost;

    impl TextInputHost for StubHost {
        fn text_input_snapshot(&self) -> TextInputSnapshot {
            TextInputSnapshot::new()
                .with_document_range(TextTargetRange::utf16_code_units(0, 5))
                .with_selection(TextSelection::utf8_bytes(5, 1))
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
                        range: ui_events::text::TextRange { start: 1, end: 5 },
                        encoding: TextRangeEncoding::Utf8Bytes,
                    },
                    TextRangeEncoding::Utf16CodeUnits,
                ) => Some(TextTargetRange::utf16_code_units(1, 3)),
                (range, encoding) if range.encoding == encoding => Some(range),
                _ => None,
            }
        }
    }

    impl TextRangeProvider for StubHost {
        fn text_for_range(&self, range: TextTargetRange) -> Option<TextRangeSlice> {
            (range == TextTargetRange::utf16_code_units(0, 5)).then(|| {
                TextRangeSlice::new("🙂", TextTargetRange::utf16_code_units(1, 3))
                    .expect("valid adjusted slice")
            })
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
            (range == TextTargetRange::utf16_code_units(0, 5)).then(|| {
                TextRangeRect::new(
                    TextTargetRange::utf16_code_units(1, 3),
                    TextInputRect::new(
                        PhysicalPosition::new(10.0, 20.0),
                        PhysicalSize::new(12.0, 18.0),
                    ),
                )
            })
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

    #[test]
    fn marked_and_directional_selected_ranges_are_exposed_in_utf16() {
        let host = StubHost;
        assert!(has_marked_text_from_host(&host));
        assert_eq!(marked_range_from_host(&host), NSRange::new(1, 2));
        assert_eq!(selected_range_from_host(&host), NSRange::new(1, 2));
    }

    #[test]
    fn substring_and_rect_queries_report_adjusted_actual_ranges() {
        let host = StubHost;
        let mut actual_range = NSRange::new(0, 0);
        let substring = attributed_substring_for_proposed_range_from_host(
            &host,
            NSRange::new(0, 5),
            Some(&mut actual_range),
        )
        .expect("substring should be present");
        assert_eq!(substring.string().to_string(), "🙂");
        assert_eq!(actual_range, NSRange::new(1, 2));

        actual_range = NSRange::new(0, 0);
        let rect = first_rect_for_character_range_from_host(
            &host,
            NSRange::new(0, 5),
            Some(&mut actual_range),
            |rect| {
                NSRect::new(
                    NSPoint::new(rect.origin.x, rect.origin.y),
                    NSSize::new(rect.size.width, rect.size.height),
                )
            },
        )
        .expect("rect should be present");
        assert_eq!(
            rect,
            NSRect::new(NSPoint::new(10.0, 20.0), NSSize::new(12.0, 18.0))
        );
        assert_eq!(actual_range, NSRange::new(1, 2));
    }

    #[test]
    fn exact_hit_testing_returns_not_found_outside_text() {
        let host = StubHost;
        assert_eq!(
            character_index_for_point_from_host(&host, NSPoint::new(1.0, 2.0), |_point| {
                PhysicalPosition::new(12.0, 24.0)
            }),
            2
        );
        assert_eq!(
            character_index_for_point_from_host(&host, NSPoint::new(100.0, 200.0), |_point| {
                PhysicalPosition::new(120.0, 240.0)
            }),
            usize::MAX
        );
    }
}
