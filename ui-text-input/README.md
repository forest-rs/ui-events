<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=ui-text-input --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs should be evaluated here.
See https://linebender.org/blog/doc-include/ for related discussion. -->
<!-- cargo-rdme start -->

Host-side text-input state and query traits for backend adapters.

This crate is the reverse side of [`ui-events`]: events carry editing intent
from a platform into an application, while these traits let a platform
adapter synchronously query coherent editor state.

The boundary is deliberately small:

- [`TextInputSnapshot`] reports selection and composition state.
- [`TextRangeProvider`] and [`SurroundingTextProvider`] expose document text.
- Geometry and hit-testing are optional capabilities with explicit semantics.
- Backend-specific lifecycle, transactions, locks, and notifications stay in
  backend adapters.

That split accommodates AppKit and UIKit queries without forcing Wayland's
double-buffering, Android's input-connection lifecycle, or Windows TSF's
document locks into every editor.

[`ui-events`]: https://docs.rs/ui-events/

<!-- cargo-rdme end -->

## Minimum supported Rust Version (MSRV)

This version of UI Text Input has been verified to compile with **Rust 1.85**
and later.

Future versions of UI Text Input might increase the Rust version requirement.

## License

Licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual-licensed as above, without any additional terms or conditions.
