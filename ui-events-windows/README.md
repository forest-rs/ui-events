<!-- We use cargo-rdme to update the README with the contents of lib.rs.
To edit the following section, update it in lib.rs, then run:
cargo rdme --workspace-project=ui-events-windows --heading-base-level=0
Full documentation at https://github.com/orium/cargo-rdme -->

<!-- Intra-doc links used in lib.rs should be evaluated here.
See https://linebender.org/blog/doc-include/ for related discussion. -->
<!-- cargo-rdme start -->

Value adapters between Windows Text Services Framework (TSF) and
[`ui-events`].

TSF's `ITextStoreACP` API uses signed `LONG` offsets into a UTF-16 stream.
This crate owns the portable, allocation-safe translation at that seam:

- coherent host state becomes ACP document and selection snapshots;
- `GetText`-style requests can split surrogate pairs without manufacturing
  invalid Rust strings;
- TSF selections preserve their active end and interim-character state;
- text replacements produce both [`TextInputEvent`] and `TS_TEXTCHANGE`
  values;
- screen-space extent and hit-test values have explicit semantics.

This is not an `ITextStoreACP` COM implementation. The native Windows layer
must still own COM identity, `AdviseSink`, `RequestLock` arbitration,
reentrancy, edit/layout notifications, thread/document/context managers,
focus, and composition sinks. See `NATIVE_INTEGRATION.md` in this crate for
the implementation checklist. Those lifetime and transaction rules cannot
be made portable without weakening TSF's contract.

[`ui-events`]: https://docs.rs/ui-events/
[`TextInputEvent`]: ui_events::text::TextInputEvent

<!-- cargo-rdme end -->

## Native integration

See [the native TSF integration checklist](NATIVE_INTEGRATION.md) before
implementing the COM text store.

## Minimum supported Rust Version (MSRV)

This version of UI Events Windows has been verified to compile with **Rust 1.85**
and later.

Future versions of UI Events Windows might increase the Rust version requirement.

## License

Licensed under either of

* Apache License, Version 2.0 or
* MIT license

at your option.
