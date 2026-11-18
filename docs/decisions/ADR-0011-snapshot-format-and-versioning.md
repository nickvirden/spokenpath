# ADR-0011: Snapshot format and versioning

## Status

Accepted

## Date

2026-11-17

## Context

The snapshot is the contract between capture and the Rust core
([ADR-0002](ADR-0002-browser-ground-truth-and-rust-core.md)). Several capture backends write it:
Chromium first, then two Safari routes
([ADR-0006](ADR-0006-engine-scope.md), [ADR-0009](ADR-0009-safari-capture-route-and-source-locations.md)).
Committed snapshots are read years later as render-golden fixtures
([ADR-0007](ADR-0007-golden-test-strategy-and-drift-reports.md)), so the core has to tell a
snapshot it can read from one it cannot. Source excerpts are taken by span from the original bytes
([ADR-0003](ADR-0003-source-tagging-by-splicing.md)), and parse5 reports offsets in UTF-16 code
units, so the units have to be named. The Chromium probes showed that the core needs raw
`ignoredReasons`, the partial-tree records for elements missing from the full tree, and the
computed `display` and `visibility` from `DOMSnapshot.captureSnapshot`
([ADR-0008](ADR-0008-chromium-protocol-behaviours.md)). The capture backends are written in
TypeScript and the core in Rust, so one shape has to be described in both languages without the
two drifting.

## Decision

1. **Envelope plus tagged payload.** A snapshot is an envelope that every engine shares (`schema`,
   `version`, `producer`, `capture`, `volatile`, `sources`, `spans`, `diagnostics`) and a `payload`
   tagged by `engine` (`chromium`, `safari-webdriver`, `safari-ax`). `capture.engine` must equal
   `payload.engine`; the reader rejects a snapshot where they differ.
2. **Span units.** Span ranges are half-open UTF-8 byte offsets into the source file. `line` is
   1-based. `column` is 1-based and counts Unicode scalar values from the start of the line.
3. **Version rule.** `version` is `{ major, minor }`. A reader rejects a major version it does not
   know, accepts any minor version, and ignores fields it does not know. A minor version only adds
   fields; anything else is a new major version. The major version starts at 0 and the schema file
   is named for it (`schema/snapshot.v0.json`).
4. **Unknown accessibility fields are preserved.** The accessibility nodes are a typed subset of the
   protocol's `AXNode`, in the protocol's own names. Every field the types do not name, at the
   node, value, value-source and related-node level, is kept and written back unchanged, so a
   protocol addition survives a round trip through the core.
5. **Raw DOM snapshot.** The Chromium payload carries the `DOMSnapshot.captureSnapshot` response as
   an optional, untyped JSON object, as delivered. The core, not capture, joins it to the DOM.
6. **Rust owns the schema.** The Rust types in `spokenpath_snapshot` define the format. The JSON
   Schema is generated from them by `cargo xtask schema`, and the TypeScript types and validator
   are generated from the committed schema. CI fails when either generated file differs from what
   its source produces.
7. **Optional means absent.** An optional field is omitted when it has no value, never written as
   `null`, and the schema does not allow `null` for it.

## Consequences

- A capture backend gets typed TypeScript and a validator that reports a failing location before
  the core ever reads the snapshot; the Rust reader reports the failing field by path.
- Committed snapshots stay readable across minor versions, and a reader fails clearly on a major
  version it predates.
- Each Safari payload is added as a minor version, by giving its variant fields.
- Unknown accessibility fields make snapshots larger than the typed subset needs, in exchange for
  losing nothing the browser exposed.
- The instrumentation package converts parse5's UTF-16 offsets to UTF-8 bytes and computes columns
  in scalar values.
- Adding a value to an enum, such as a new DOM node kind or element origin, makes a snapshot an
  older reader rejects, so it is a new major version unless the field stays a string.
- A whole-number float written from JavaScript, such as a device pixel ratio of 2, is an integer
  in JSON and comes back from Rust as `2.0`; the values are equal, the text is not.

## Rejected alternatives

- **A payload selected by `capture.engine` alone, with no tag of its own.** The schema and the
  TypeScript types could not express a discriminated union, so a writer could pair a Chromium
  envelope with any payload.
- **Dropping `capture.engine` and tagging only the payload.** It keeps one source of truth but
  removes the engine from the envelope, where the capture conditions are recorded together.
- **Columns in UTF-16 code units or in bytes.** UTF-16 is parse5's unit, not a reader's; bytes do
  not match what an editor shows. The ranges already carry byte precision.
- **Rejecting unknown fields.** Every minor addition would break every older reader.
- **Typing the whole `DOMSnapshot` response, or joining its layout data into the DOM nodes at
  capture.** Typing it adds a large surface the core reads only in part; joining it is
  interpretation in capture, which ADR-0002 rules out.
- **A hand-written JSON Schema or TypeScript-first types.** Two hand-maintained descriptions of one
  contract drift; generating from the reader's own types keeps the reader authoritative.
