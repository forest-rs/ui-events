<div align="center">

# UI Events AppKit Adapter

A library for bridging AppKit events into the `ui-events` model.

[![Linebender Zulip, #general channel](https://img.shields.io/badge/Linebender-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)
[![dependency status](https://deps.rs/repo/github/endoli/ui-events/status.svg)](https://deps.rs/repo/github/endoli/ui-events)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
[![Build status](https://github.com/endoli/ui-events/workflows/CI/badge.svg)](https://github.com/endoli/ui-events/actions)
[![Crates.io](https://img.shields.io/crates/v/ui-events-appkit.svg)](https://crates.io/crates/ui-events-appkit)
[![Docs](https://docs.rs/ui-events-appkit/badge.svg)](https://docs.rs/ui-events-appkit)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=ui-events-appkit --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs should be evaluated here.
See https://linebender.org/blog/doc-include/ for related discussion. -->
[`ui-events`]: https://docs.rs/ui-events/
<!-- cargo-rdme start -->

AppKit (macOS) adapter using `objc2`.

This crate provides lightweight helpers to convert AppKit events into
[`ui-events`] types, mirroring the style of the `ui-events-web` and
`ui-events-winit` adapters.

It does not provide an `NSView` implementation for you. Instead, your
AppKit view or responder should call these helpers from the corresponding
AppKit callbacks.

Currently supported:

- Pointer (mouse/pen) down/up/move/enter/leave/scroll
- Trackpad gestures: magnify (pinch) and rotate
- Keyboard down/up
- Text-input mapping helpers for committed text, composition updates, and
  UTF-16 replacement ranges
- `AppKitInputResponder`, a reusable `NSResponder` for pointer, scroll,
  gesture, tablet, and keyboard input

`AppKitInputResponder` intentionally does not implement text input, IME, or
edit-command protocols.

## Feature Policy

- `std` is enabled by default.
- `libm` is required for `no_std` builds because tablet tilt orientation uses
  trigonometry.
- With `default-features = false`, the crate remains `no_std` with `alloc`
  and still compiles on non-macOS targets because the `objc2` dependencies
  are behind `target_os = "macos"`.
- On macOS targets, the `objc2*` crates are compiled with their `std` feature
  enabled to support Objective-C runtime integration.
- Defaults avoid pulling in unnecessary APIs by disabling default features
  for `objc2*` crates and enabling only the AppKit symbols this crate uses.

## Pointer Notes

- Pointer positions come from `NSEvent::locationInWindow`, scaled from
  points into physical pixels but not converted into a specific `NSView`.
  `pointer_event_from_nsevent_at_position` accepts a caller-provided
  coordinate, and `AppKitInputResponderHost` requires the host to provide
  the position in its own coordinate space.
- Event timestamps use AppKit's monotonic seconds-since-boot timebase, not
  Unix epoch time.
- Mouse button down/up click counts use AppKit's `clickCount` directly.
  Other pointer event kinds, including scroll events, use `0` without
  querying click metadata from AppKit.
- Mouse events may query AppKit tablet metadata when AppKit reports tablet
  mouse-event subtypes. Scroll events do not query tablet metadata.
- Tablet tilt uses AppKit's `NSEvent::tilt` fractions, where `0.0` is
  perpendicular and `1.0` is parallel to the surface along that axis.

## Scroll Deltas

- Scroll deltas preserve AppKit's `scrollingDeltaX/Y` sign, which already
  reflects the user's natural-scrolling preference and matches the macOS
  path used by `winit`.
- When `hasPreciseScrollingDeltas` is true, deltas map to
  [`ScrollDelta::PixelDelta`]. Otherwise, they map to
  [`ScrollDelta::LineDelta`].

## Gestures

- Magnify gesture events map to [`PointerGesture::Pinch`](ui_events::pointer::PointerGesture::Pinch)
  using AppKit's per-event `magnification` delta.
- Rotate gesture events map to
  [`PointerGesture::Rotate`](ui_events::pointer::PointerGesture::Rotate) in
  clockwise radians.

## Tablet Events

- `NSEventType::TabletPoint` maps to [`PointerType::Pen`] with pressure,
  tangential pressure, and orientation derived from AppKit tilt fractions.
- `NSEventType::TabletProximity` maps to `PointerEvent::Enter` or
  `PointerEvent::Leave` based on `isEnteringProximity`.

## Keyboard Notes

- `flagsChanged` uses aggregate modifier flags. Releasing one side of a
  modifier while the other side remains held may still report the key as
  down until this adapter grows device-dependent left/right modifier masks.

## Text Notes

- [`text`] contains value-based helpers for translating AppKit UTF-16
  location/length pairs and text callbacks into [`TextInputEvent`] values.
- The reusable `AppKitInputResponder` still does not implement `NSTextInput`
  protocols. Hosts that implement those protocols can use the text helpers
  from their own responder or view.

## High-Level Helpers

- `AppKitInputResponder`
- `pointer_event_from_nsevent`
- `pointer_event_from_nsevent_at_position`
- `keyboard_event_from_nsevent`
- `text::text_insert_event`
- `text::composition_update_event_with_utf16_ranges`

If you prefer, low-level mappers in [`mapping`] let you build events from
raw values (e.g. coordinates, button number, modifier booleans) without
pulling in AppKit types in your own code.

[`PointerType::Pen`]: ui_events::pointer::PointerType::Pen
[`ScrollDelta::LineDelta`]: ui_events::ScrollDelta::LineDelta
[`ScrollDelta::PixelDelta`]: ui_events::ScrollDelta::PixelDelta
[`TextInputEvent`]: ui_events::text::TextInputEvent
[`ui-events`]: https://docs.rs/ui-events/

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This version of UI Events AppKit has been verified to compile with **Rust 1.85** and later.

Future versions of UI Events AppKit might increase the Rust version requirement.
It will not be treated as a breaking change and as such can even happen with small patch releases.

<details>
<summary>Click here if compiling fails.</summary>

As time has passed, some UI Events AppKit dependencies could have released versions with a higher Rust requirement.
If you encounter a compilation issue due to a dependency and don't want to upgrade your Rust toolchain, then you could downgrade the dependency.

```sh
# Use the problematic dependency's name and version
cargo update -p package_name --precise 0.1.1
```

</details>

## Community

[![Linebender Zulip](https://img.shields.io/badge/Xi%20Zulip-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)

Discussion of UI Events AppKit development happens in the [Linebender Zulip](https://xi.zulipchat.com/), specifically the [#general channel](https://xi.zulipchat.com/#narrow/channel/147921-general).
All public content can be read without logging in.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Contributions are welcome by pull request. The [Rust code of conduct] applies.
Please feel free to add your name to the [AUTHORS] file in any substantive pull request.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be licensed as above, without any additional terms or conditions.

[Rust Code of Conduct]: https://www.rust-lang.org/policies/code-of-conduct
[AUTHORS]: ./AUTHORS
