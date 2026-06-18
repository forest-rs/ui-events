<!-- Instructions

This changelog follows the patterns described here: <https://keepachangelog.com/en/>.

Subheadings to categorize changes are `added, changed, deprecated, removed, fixed, security`.

-->

# Changelog

UI Events Wayland has not yet been published.

## [Unreleased]

This release has an [MSRV][] of 1.85.

### Added

- Platform-neutral Wayland input mapping helpers in the `mapping` module:
  evdev pointer-button mapping, surface-local coordinate scaling, scroll-axis
  frame to `ScrollDelta` conversion, pointer and touch identity helpers, touch
  contact-geometry and orientation conversions, and modifier helpers.
- `pointer::PointerEventReducer`, which reduces a `wl_pointer` event stream into
  `PointerEvent`s, accumulating frame-batched scroll axes, tracking button and
  click-count state, and stamping a caller-provided monotonic timestamp.
- `touch::TouchEventReducer`, which reduces a `wl_touch` event stream into touch
  `PointerEvent`s, tracking each contact's state, frame-batching shape and
  orientation updates, mapping the lowest active contact to the primary pointer,
  and stamping a caller-provided monotonic timestamp.

[Unreleased]: https://github.com/endoli/ui-events/compare/v0.3.0...HEAD

[MSRV]: README.md#minimum-supported-rust-version-msrv
