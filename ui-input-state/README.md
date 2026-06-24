<div align="center">

# UI Input State

A library for stateful tracking of current input state.

[![Linebender Zulip, #general channel](https://img.shields.io/badge/Linebender-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)
[![dependency status](https://deps.rs/repo/github/endoli/ui-events/status.svg)](https://deps.rs/repo/github/endoli/ui-events)
[![Apache 2.0 or MIT license.](https://img.shields.io/badge/license-Apache--2.0_OR_MIT-blue.svg)](#license)
[![Build status](https://github.com/endoli/ui-events/workflows/CI/badge.svg)](https://github.com/endoli/ui-events/actions)
[![Crates.io](https://img.shields.io/crates/v/ui-input-state.svg)](https://crates.io/crates/ui-input-state)
[![Docs](https://docs.rs/ui-input-state/badge.svg)](https://docs.rs/ui-input-state)

</div>

<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=ui-input-state --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs should be evaluated here. 
See https://linebender.org/blog/doc-include/ for related discussion. -->
<!-- cargo-rdme start -->

Frame-oriented input state built on `ui-events`.

This crate provides simple state containers to make input handling easier in
immediate-mode or frame-based UIs. Instead of reacting to each event
individually, you feed pointer and keyboard events into the state, query the
current and per-frame information during your update, and then call
[`InputState::clear_frame`](https://docs.rs/ui-input-state/latest/ui_input_state/input_state/struct.InputState.html#method.clear_frame) at the end of the frame.

## What it provides:

- [`PrimaryPointerState`](https://docs.rs/ui-input-state/latest/ui_input_state/primary_pointer_state/struct.PrimaryPointerState.html): current pointer state, coalesced and predicted motion,
  per-frame button transitions, and helpers for motion in physical/logical units.
- [`KeyboardState`](https://docs.rs/ui-input-state/latest/ui_input_state/keyboard_state/struct.KeyboardState.html): current modifiers, keys down, and per-frame key transitions.
- [`InputState`](https://docs.rs/ui-input-state/latest/ui_input_state/input_state/struct.InputState.html): a convenience container bundling both states and a per-frame clear.

## Typical lifecycle per frame:

1. Receive backend events and convert them to `ui-events` types.
2. Update `PrimaryPointerState` and `KeyboardState` with the events.
3. Read state during your UI update (e.g. check just pressed, motion, etc.).
4. Call [`InputState::clear_frame`](https://docs.rs/ui-input-state/latest/ui_input_state/input_state/struct.InputState.html#method.clear_frame) before the next frame.

## Example (sketch):

```rust
use ui_input_state::{InputState, PrimaryPointerState, KeyboardState};
use ui_events::pointer::PointerEvent;
use ui_events::keyboard::KeyboardEvent;

let mut input = InputState::default();

// 1-2) In your event loop, feed events into state
fn on_pointer_event(input: &mut InputState, e: PointerEvent) {
    input.primary_pointer.process_pointer_event(e);
}
fn on_keyboard_event(input: &mut InputState, e: KeyboardEvent) {
    input.keyboard.process_keyboard_event(e);
}

// 3) During your update pass, read state
fn update(input: &InputState) {
    if input.primary_pointer.is_primary_just_pressed() {
        // Begin a drag, for example
    }
    if input.keyboard.key_str_just_pressed("z") && input.keyboard.modifiers.ctrl() {
        // Ctrl+Z
    }
}

// 4) At the end of the frame, clear per-frame transitions
fn end_frame(input: &mut InputState) { input.clear_frame(); }
```

## Coordinates and units

Pointer positions are stored in physical pixels with a Y-down axis, as in
`ui-events`. Use [`PrimaryPointerState::current_logical_position`](https://docs.rs/ui-input-state/latest/ui_input_state/primary_pointer_state/struct.PrimaryPointerState.html#method.current_logical_position) and
[`PrimaryPointerState::logical_motion`](https://docs.rs/ui-input-state/latest/ui_input_state/primary_pointer_state/struct.PrimaryPointerState.html#method.logical_motion) to work in logical units.

## Features

- `std` (enabled by default): Use the Rust standard library.
- `libm`: Enable `ui-events/libm` transitively for `no_std` environments.

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This version of UI Input State has been verified to compile with **Rust 1.85** and later.

Future versions of UI Input State might increase the Rust version requirement.
It will not be treated as a breaking change and as such can even happen with small patch releases.

<details>
<summary>Click here if compiling fails.</summary>

As time has passed, some of UI Input State's dependencies could have released versions with a higher Rust requirement.
If you encounter a compilation issue due to a dependency and don't want to upgrade your Rust toolchain, then you could downgrade the dependency.

```sh
# Use the problematic dependency's name and version
cargo update -p package_name --precise 0.1.1
```

</details>

## Community

[![Linebender Zulip](https://img.shields.io/badge/Linebender%20Zulip-%23general-blue?logo=Zulip)](https://xi.zulipchat.com/#narrow/channel/147921-general)

Discussion of UI Input State development happens in the [Linebender Zulip](https://xi.zulipchat.com/), specifically the [#general channel](https://xi.zulipchat.com/#narrow/channel/147921-general).
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
