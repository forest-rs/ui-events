# Native TSF integration checklist

`ui-events-windows` deliberately stops at values. A production Windows bridge
still needs all of the following; none is optional merely because the portable
queries compile.

## COM and thread lifetime

- Implement one stable COM identity for `IUnknown`, `ITextStoreACP` (or
  `ITextStoreACP2`), and any composition/view interfaces the store exposes.
- Keep the store on the UI thread that owns its window and editor.
- Activate `ITfThreadMgr`, create an `ITfDocumentMgr` and context, push the
  context, and associate focus with the focused editor window.
- Pop/release contexts and deactivate the thread manager in strict reverse
  order. Invalidate callbacks before releasing the editor.

## Lock state machine

- Implement `RequestLock` as a state machine, not a mutex wrapper.
- Grant the requested lock only for the dynamic extent of
  `ITextStoreACPSink::OnLockGranted`.
- Allow a read lock to queue one asynchronous read/write upgrade. Coalesce
  queued requests to the strongest access and never grant a write lock
  reentrantly.
- Return the session result separately from the `RequestLock` call result.
- Reject read methods without a read lock and mutation methods without a
  read/write lock.
- Do not mutate the document or send `OnTextChange` from inside
  `RequestLock`; drain application-originated notifications after the lock.

## Sink and notifications

- Support one advised `ITextStoreACPSink`, validate its interface identity,
  retain its mask, and balance `AdviseSink`/`UnadviseSink` references.
- Send `OnTextChange`, `OnSelectionChange`, and `OnLayoutChange` only when the
  advised mask requests them.
- Do not echo `OnTextChange` for the exact mutation TSF made through `SetText`
  or `InsertTextAtSelection`; return `TS_TEXTCHANGE` from that call instead.
- Treat layout availability independently from document availability and
  return `TS_E_NOLAYOUT` while geometry is stale.

## Text-store methods

- Serve `GetText` in ACP UTF-16 units, including buffer boundaries between
  surrogate halves. `TsfDocumentSnapshot::get_text` covers this value rule.
- Preserve the active selection end and interim-character bit in
  `GetSelection` and `SetSelection`.
- Validate every ACP position against the same locked revision used for the
  operation. Report read-only and region-boundary failures explicitly.
- Implement `QueryInsert`, plain-text runs, status flags, view cookies,
  `GetScreenExt`, `GetTextExt`, and `GetACPFromPoint` with physical virtual-
  screen coordinates. Multi-monitor coordinates can be negative.
- Decide which rich-text, embedded-object, and attribute methods are genuinely
  supported. Return the documented unsupported result for the rest.

## Composition and focus

- Implement the composition sink required by the chosen TSF context model and
  keep its range synchronized with the editor composition snapshot.
- Terminate or transfer composition deliberately when focus changes, the
  editor becomes read-only, or the backing document disappears.
- Keep candidate-window geometry current on caret movement, scrolling, window
  movement, DPI changes, and monitor changes.

## Validation matrix

- Exercise the Microsoft Japanese, Simplified Chinese, Traditional Chinese,
  and Korean IMEs on supported Windows versions.
- Cover emoji and supplementary-plane text, reversed selections, interim
  characters, reconversion/correction, candidate placement, partial `GetText`
  buffers, and asynchronous lock upgrades.
- Test focus churn, editor destruction during callbacks, nested window loops,
  per-monitor DPI, negative monitor coordinates, touch keyboard, pen
  handwriting, and accessibility clients querying the same text store.
