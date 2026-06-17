<div align="center">

# UI Events

A library for working with UI events and input types.

[![Linebender Zulip, #general channel](https://img.shields.io/badge/Linebender-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)
[![dependency status](https://deps.rs/repo/github/endoli/ui-events/status.svg)](https://deps.rs/repo/github/endoli/ui-events)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
[![Build status](https://github.com/endoli/ui-events/workflows/CI/badge.svg)](https://github.com/endoli/ui-events/actions)
[![Crates.io](https://img.shields.io/crates/v/ui-events.svg)](https://crates.io/crates/ui-events)
[![Docs](https://docs.rs/ui-events/badge.svg)](https://docs.rs/ui-events)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=ui-events --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs should be evaluated here. 
See https://linebender.org/blog/doc-include/ for related discussion. -->
<!-- cargo-rdme start -->

Cross-platform input event types modeled after W3C UI Events.

This crate provides small, portable data types for working with pointer
(mouse, touch, pen) and keyboard input in a platform-agnostic way.
It aims to closely follow W3C terminology while remaining practical for native
application development.

## What you get:

- Pointer events: button down/up, move, enter/leave, scroll, gestures
- Rich pointer state: position, pressure, tilt, contact size, modifiers
- Keyboard types re-exported from [`keyboard-types`]
- Text-input events for soft keyboards and IMEs
- Edit-command events for semantic editor operations
- A stable vocabulary you can adapt from windowing backends

This crate is intentionally focused on data structures — it does not open
windows or read events.
For integrations, see the adapter crates:

- [`ui-events-winit`]: Convert between `winit` and `ui-events`.
- [`ui-events-web`]: Convert between Web (`web-sys`) DOM events and `ui-events`.

## Coordinate system and units

- Positions are in physical pixels (`dpi::PhysicalPosition<f64>`), with the
  Y axis increasing downward.
- Use [`PointerState::logical_position`](pointer::PointerState::logical_position)
  to obtain logical coordinates using a scale factor.
- Scroll deltas are expressed via [`ScrollDelta`]; see its docs for details
  on page/line/pixel semantics.

## Primary pointer

Some interactions need a notion of a “primary” pointer (e.g. left mouse button, first touch).
The reserved id [`PointerId::PRIMARY`](pointer::PointerId::PRIMARY) marks this.
Helper methods like [`PointerEvent::is_primary_pointer`](pointer::PointerEvent::is_primary_pointer)
and [`PointerInfo::is_primary_pointer`](pointer::PointerInfo::is_primary_pointer) are provided for convenience.

## Feature flags

- `std` (default): Use the Rust standard library.
- `kurbo`: Add convenience methods for converting positions to `kurbo::Point`.

## Examples

Basic matching on pointer events:

```rust
use ui_events::pointer::{PointerEvent, PointerButton, PointerButtonEvent, PointerInfo, PointerState, PointerType};
use ui_events::ScrollDelta;
use keyboard_types::Modifiers;
use dpi::{PhysicalPosition, PhysicalSize};

fn handle_event(ev: PointerEvent) {
    match ev {
        PointerEvent::Down(PointerButtonEvent { button, state, .. }) => {
            if button == Some(PointerButton::Primary) {
                // Start a drag, for example
                let pos = state.position;
                let _ = (pos.x, pos.y);
            }
        }
        PointerEvent::Move(upd) => {
            let logical = upd.current.logical_position();
            let _ = (logical.x, logical.y);
        }
        PointerEvent::Scroll(s) => {
            match s.delta {
                ScrollDelta::PageDelta(x, y) => { let _ = (x, y); }
                ScrollDelta::LineDelta(x, y) => { let _ = (x, y); }
                ScrollDelta::PixelDelta(p) => { let _ = (p.x, p.y); }
            }
        }
        _ => {}
    }
}

// Construct a minimal primary-pointer Down event
let ev = PointerEvent::Down(PointerButtonEvent{
    button: Some(PointerButton::Primary),
    pointer: PointerInfo{
        pointer_id: Some(ui_events::pointer::PointerId::PRIMARY),
        persistent_device_id: None,
        pointer_type: PointerType::Mouse,
    },
    state: PointerState{
        time: 0,
        position: PhysicalPosition { x: 10.0, y: 20.0 },
        buttons: Default::default(),
        modifiers: Modifiers::empty(),
        count: 1,
        contact_geometry: PhysicalSize { width: 1.0, height: 1.0 },
        orientation: Default::default(),
        pressure: 0.5,
        tangential_pressure: 0.0,
        scale_factor: 2.0,
    },
});
handle_event(ev);
```

## See also

- [`ui-events-winit`]
- [`ui-events-web`]
- [`keyboard-types`]

[`keyboard-types`]: https://docs.rs/keyboard-types/
[`ui-events-winit`]: https://docs.rs/ui-events-winit/
[`ui-events-web`]: https://docs.rs/ui-events-web/
[`winit`]: https://docs.rs/winit/

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This version of UI Events has been verified to compile with **Rust 1.85** and later.

Future versions of UI Events might increase the Rust version requirement.
It will not be treated as a breaking change and as such can even happen with small patch releases.

<details>
<summary>Click here if compiling fails.</summary>

As time has passed, some of UI Events' dependencies could have released versions with a higher Rust requirement.
If you encounter a compilation issue due to a dependency and don't want to upgrade your Rust toolchain, then you could downgrade the dependency.

```sh
# Use the problematic dependency's name and version
cargo update -p package_name --precise 0.1.1
```

</details>

## Community

[![Linebender Zulip](https://img.shields.io/badge/Xi%20Zulip-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)

Discussion of UI Events development happens in the [Linebender Zulip](https://xi.zulipchat.com/), specifically the [#general channel](https://xi.zulipchat.com/#narrow/channel/147921-general).
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
