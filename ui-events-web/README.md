<div align="center">

# UI Events for Web

A library for bridging [`web-sys`] events into the [`ui-events`] model.

[![Linebender Zulip, #general channel](https://img.shields.io/badge/Linebender-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)
[![dependency status](https://deps.rs/repo/github/endoli/ui-events/status.svg)](https://deps.rs/repo/github/endoli/ui-events)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
[![Build status](https://github.com/endoli/ui-events/workflows/CI/badge.svg)](https://github.com/endoli/ui-events/actions)
[![Crates.io](https://img.shields.io/crates/v/ui-events-web.svg)](https://crates.io/crates/ui-events-web)
[![Docs](https://docs.rs/ui-events-web/badge.svg)](https://docs.rs/ui-events-web)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=ui-events-web --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs should be evaluated here.
See https://linebender.org/blog/doc-include/ for related discussion. -->
[`ui-events`]: https://docs.rs/ui-events/
[`web-sys`]: https://docs.rs/web-sys/
<!-- cargo-rdme start -->

This crate bridges [`web_sys`] DOM input events — Pointer Events (mouse, touch, pen),
Wheel, and Keyboard — into the [`ui-events`] model.

It provides lightweight helpers to convert browser events into portable
`ui-events` types you can feed into your input handling. It supports
Pointer Events (mouse, touch, pen), keyboard, and text/composition input.

## Keyboard

- [`keyboard::from_web_keyboard_event`]
- Optional helpers: [`keyboard::from_web_keydown_event`], [`keyboard::from_web_keyup_event`]

## Text / Composition

- [`text::text_event_from_dom_event`]
- Typed helpers: [`text::from_web_input_event`], [`text::from_web_composition_event`]

## Pointer (Pointer Events)

- One‑shot DOM conversion: [`pointer::pointer_event_from_dom_event`]
- Multi-touch aware DOM conversion (may return multiple events):
  [`pointer::pointer_events_from_dom_event`]
- Per‑event helpers (preferred):
  [`pointer::down_from_pointer_event`], [`pointer::up_from_pointer_event`],
  [`pointer::move_from_pointer_event`], [`pointer::enter_from_pointer_event`],
  [`pointer::leave_from_pointer_event`], [`pointer::cancel_from_pointer_event`]
- Mouse‑only helpers (legacy and less portable):
  [`pointer::down_from_mouse_event`], [`pointer::up_from_mouse_event`],
  [`pointer::move_from_mouse_event`], [`pointer::enter_from_mouse_event`],
  [`pointer::leave_from_mouse_event`], [`pointer::scroll_from_wheel_event`]
- Conversion options: [`pointer::Options`] (controls scale/coalesced/predicted)
- Pointer capture helpers: [`pointer::set_pointer_capture`],
  [`pointer::release_pointer_capture`], [`pointer::has_pointer_capture`]

## Notes

- Positions use `clientX` / `clientY` scaled by `Options::scale_factor`. Pass the
  current device-pixel-ratio for physical pixels.
- **Element-local coordinates (recipe):** DOM pointer coordinates are reported in viewport
  space even when you register the listener on an element. For canvas/SVG apps you often
  want coordinates relative to a specific element’s top-left corner. A common recipe is:

  ```rust
  let rect = el.get_bounding_client_rect();
  let dpr = window().unwrap().device_pixel_ratio();
  let x = (e.client_x() as f64 - rect.left()) * dpr;
  let y = (e.client_y() as f64 - rect.top()) * dpr;
  ```

  This does *not* undo arbitrary CSS transforms, and for SVG you may want a true transform
  inversion (e.g. `getScreenCTM()`).
- Coalesced and predicted move samples are opt‑in via `Options`.
- Touch events (`touchstart`/`touchmove`/`touchend`/`touchcancel`) may correspond to multiple
  changed touches; use `pointer_events_from_dom_event` to receive all of them.
- Stylus fields:
  - `pressure`, `tangential_pressure`, `contact_geometry`, and `orientation` are populated
    from Pointer Events data when available (preferring `altitudeAngle`/`azimuthAngle`,
    otherwise falling back to `tiltX`/`tiltY`).
  - Stylus rotation/twist (Pointer Events `twist`) is not currently exposed by `ui-events`,
    so it is not mapped.
- Keyboard: unknown `key`/`code` map to `Unidentified`; `is_composing` reflects the DOM flag.
- Text:
  - `CompositionEvent` maps to composition update/end.
  - `InputEvent` maps committed insertions and simple backward/forward deletion intents.
  - DOM target ranges are node-relative and are not currently converted into `ui-events`
    replacement ranges.

## Example

```rust
use web_sys::wasm_bindgen::JsCast;
use web_sys::{window, Event, KeyboardEvent};
use ui_events_web::{keyboard, pointer};

// Inside an event listener closure…
let ev: Event = /* from DOM */
let win = window().unwrap();
let opts = pointer::Options::default()
    .with_scale(win.device_pixel_ratio())
    .with_coalesced(true)
    .with_predicted(true);

if let Some(pe) = pointer::pointer_event_from_dom_event(&ev, &opts) {
    match pe {
        ui_events::pointer::PointerEvent::Move(update) => {
            // Use update.current; update.coalesced / update.predicted may be empty
        }
        ui_events::pointer::PointerEvent::Down(_) => {}
        ui_events::pointer::PointerEvent::Up(_) => {}
        _ => {}
    }
}

if let Some(ke) = ev.dyn_ref::<KeyboardEvent>() {
    let k = keyboard::from_web_keyboard_event(ke);
    // Use k.state, k.code, k.key, k.modifiers …
}
```

[`ui-events`]: https://docs.rs/ui-events/

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This version of UI Events for Web has been verified to compile with **Rust 1.85** and later.

Future versions of UI Events for Web might increase the Rust version requirement.
It will not be treated as a breaking change and as such can even happen with small patch releases.

<details>
<summary>Click here if compiling fails.</summary>

As time has passed, some of UI Events for Web's dependencies could have released versions with a higher Rust requirement.
If you encounter a compilation issue due to a dependency and don't want to upgrade your Rust toolchain, then you could downgrade the dependency.

```sh
# Use the problematic dependency's name and version
cargo update -p package_name --precise 0.1.1
```

</details>

## Community

[![Linebender Zulip](https://img.shields.io/badge/Xi%20Zulip-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)

Discussion of UI Events for Web development happens in the [Linebender Zulip](https://xi.zulipchat.com/), specifically the [#general channel](https://xi.zulipchat.com/#narrow/channel/147921-general).
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
