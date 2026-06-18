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
  contact-geometry and orientation conversions, modifier helpers, pinch
  scale-fraction and rotation conversions, and tablet-tool helpers (stylus-button
  mapping, tool-type to pointer-type and tip-button mapping, normalized pressure
  and slider conversions, and tilt-to-orientation conversion).
- `pointer::PointerEventReducer`, which reduces a `wl_pointer` event stream into
  `PointerEvent`s, accumulating frame-batched scroll axes, tracking button and
  click-count state, and stamping a caller-provided monotonic timestamp.
- `touch::TouchEventReducer`, which reduces a `wl_touch` event stream into touch
  `PointerEvent`s, tracking each contact's state, frame-batching shape and
  orientation updates, mapping the lowest active contact to the primary pointer,
  and stamping a caller-provided monotonic timestamp.
- `gesture::PinchGestureReducer`, which reduces a `zwp_pointer_gesture_pinch_v1`
  touchpad pinch event stream into pinch and rotate `PointerEvent`s, converting
  Wayland's absolute scale into a per-update scale fraction and its
  clockwise-degree rotation into radians.
- `tablet::TabletToolReducer`, which reduces a `zwp_tablet_tool_v2` event stream
  into pen `PointerEvent`s, frame-batching the tool's position and pressure,
  tilt, and slider axes, mapping the tip and stylus barrel buttons (an eraser
  tool's tip to the pen-eraser button), surfacing proximity as enter and leave,
  and stamping a caller-provided monotonic timestamp.

[Unreleased]: https://github.com/endoli/ui-events/compare/v0.3.0...HEAD

[MSRV]: README.md#minimum-supported-rust-version-msrv
