<div align="center">

# UI Events UIKit Adapter

A library for bridging UIKit events into the `ui-events` model.

[![Linebender Zulip, #general channel](https://img.shields.io/badge/Linebender-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)
[![dependency status](https://deps.rs/repo/github/endoli/ui-events/status.svg)](https://deps.rs/repo/github/endoli/ui-events)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
[![Build status](https://github.com/endoli/ui-events/workflows/CI/badge.svg)](https://github.com/endoli/ui-events/actions)
[![Crates.io](https://img.shields.io/crates/v/ui-events-uikit.svg)](https://crates.io/crates/ui-events-uikit)
[![Docs](https://docs.rs/ui-events-uikit/badge.svg)](https://docs.rs/ui-events-uikit)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=ui-events-uikit --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs should be evaluated here.
See https://linebender.org/blog/doc-include/ for related discussion. -->
[`ui-events`]: https://docs.rs/ui-events/
<!-- cargo-rdme start -->

UIKit (iOS/tvOS) adapter using `objc2`.

This crate provides lightweight helpers to convert UIKit events into
[`ui-events`] types, mirroring the style of the `ui-events-web` and
`ui-events-winit` adapters.

It does not provide a `UIView` implementation for you. Instead, your
`UIView` or responder should call these helpers from the corresponding
UIKit callbacks, or install `UIKitInputResponder` as a reusable responder
for touch, remote, and keyboard input.

Currently supported:

- Pointer (touch/stylus) down/up/move/cancel
- Pencil hover enter/move/leave when UIKit reports region phases
- tvOS remote presses → keyboard
- Hardware keyboard via `UIPress` + `UIKey`
- Text-input mapping helpers for committed text, composition updates, and
  UTF-16 replacement ranges
- `UIKitInputResponder`, a reusable `UIResponder` for touch, remote, and
  keyboard input

## Feature Policy

- `std` is enabled by default and uses the platform math intrinsics.
- `libm` is required for `no_std` builds because stylus pressure normalization uses `sin`.
- The crate is `no_std` with `alloc` when default features are disabled.
  The `objc2*` crates are still compiled with their `std` feature enabled on
  Apple mobile targets to support Objective-C runtime integration.
- Defaults avoid pulling in unnecessary APIs by disabling default features
  for `objc2*` crates and enabling only the UIKit symbols this crate uses.

## Pointer Notes

- Touch positions come from `UITouch::preciseLocationInView(None)`, scaled
  from points into physical pixels. The caller chooses the view coordinate
  space by passing the view to UIKit before using lower-level mappers.
- Touch timestamps use UIKit's monotonic seconds-since-boot timebase, not
  Unix epoch time.
- Touch pressure is derived from `UITouch.force/maximumPossibleForce` when available.
- Stylus (Apple Pencil) pressure is converted from force along the stylus axis to
  perpendicular-to-surface pressure using `sin(altitudeAngle)`.
- Stylus orientation is mapped from `altitudeAngle`/`azimuthAngleInView`.
- Pencil hover maps to enter/move/leave when UIKit reports `RegionEntered`,
  `RegionMoved`, and `RegionExited`.
- Finger touches use `button: None`, matching the DOM `TouchEvent` path in
  `ui-events-web`. Active stylus contacts use [`PointerButton::Primary`](ui_events::pointer::PointerButton::Primary).
- UIKit exposes stylus `rollAngle`, but `ui-events` currently has no roll field.
- UIKit exposes estimated-property update APIs, but `ui-events` currently has no sample-revision
  metadata for correcting previously emitted stylus samples.
- UIKit does not expose a tangential-pressure value on `UITouch`, so
  [`PointerState::tangential_pressure`](ui_events::pointer::PointerState::tangential_pressure)
  remains `0.0` for this adapter.

## Gestures

- Pinch gesture recognizers map to [`PointerGesture::Pinch`](ui_events::pointer::PointerGesture::Pinch)
  by differencing UIKit's cumulative `scale` against a caller-provided
  previous scale. Rotation gesture recognizers follow the same pattern with
  cumulative counterclockwise `rotation` values.

## Text Notes

- [`text`] contains value-based helpers for translating UIKit UTF-16
  location/length pairs and text callbacks into [`TextInputEvent`] values.
- The reusable `UIKitInputResponder` still does not implement `UIKeyInput`
  or full text-input protocols. Hosts that implement those protocols can use
  the text helpers from their own responder or view.

## High-Level Helpers

- `UIKitInputResponder`
- `keyboard_event_from_uipress`
- `keyboard_event_from_uikey`
- `text::text_insert_event`
- `text::composition_update_event_with_utf16_ranges`
- `pointer_event_from_touch_and_event`
- `pointer_event_from_touch` (uncommon convenience helper)
- `pointer_gesture_from_uipinch` (feature: `gestures`)
- `pointer_gesture_from_uirotation` (feature: `gestures`)
- `mapping::pinch_delta_from_cumulative_scale` (feature: `gestures`)
- `mapping::rotation_delta_from_cumulative_rotation` (feature: `gestures`)

If you prefer, low-level mappers in [`mapping`] let you build events from
raw values (e.g. coordinates, button number, modifier booleans) without
pulling in UIKit types in your own code.

[`ui-events`]: https://docs.rs/ui-events/
[`TextInputEvent`]: ui_events::text::TextInputEvent

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This version of UI Events UIKit has been verified to compile with **Rust 1.85** and later.

Future versions of UI Events UIKit might increase the Rust version requirement.
It will not be treated as a breaking change and as such can even happen with small patch releases.

<details>
<summary>Click here if compiling fails.</summary>

As time has passed, some UI Events UIKit dependencies could have released versions with a higher Rust requirement.
If you encounter a compilation issue due to a dependency and don't want to upgrade your Rust toolchain, then you could downgrade the dependency.

```sh
# Use the problematic dependency's name and version
cargo update -p package_name --precise 0.1.1
```

</details>

## Community

[![Linebender Zulip](https://img.shields.io/badge/Xi%20Zulip-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)

Discussion of UI Events UIKit development happens in the [Linebender Zulip](https://xi.zulipchat.com/), specifically the [#general channel](https://xi.zulipchat.com/#narrow/channel/147921-general).
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
