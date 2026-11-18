# Snapshot format

A snapshot is one JSON file holding what the browser exposes for one page in one engine, raw and
uninterpreted ([ADR-0002](decisions/ADR-0002-browser-ground-truth-and-rust-core.md)). Capture
writes it and the Rust core reads it. The decisions behind the format are in
[ADR-0011](decisions/ADR-0011-snapshot-format-and-versioning.md).

The Rust types in `crates/spokenpath_snapshot` define the format. Everything else is generated
from them:

- `schema/snapshot.v0.json`: the JSON Schema (draft 2020-12), from `cargo xtask schema`.
- `packages/snapshot-types/src/snapshot.generated.ts`: the TypeScript types, from
  `pnpm --filter @spokenpath/snapshot-types generate`.

`@spokenpath/snapshot-types` also exports `validateSnapshot(value)`, which checks a value against
the committed schema and returns `{ ok: true, snapshot }` or `{ ok: false, errors }`, each error
carrying the JSON Pointer of the failing location.

## Envelope

| Field | Contents |
|---|---|
| `schema` | Always `"spokenpath-snapshot"`. |
| `version` | `{ major, minor }`. Writers write `{ "major": 0, "minor": 0 }`. |
| `producer` | `{ name, version }` of the program that wrote the snapshot. |
| `capture.engine` | `chromium`, `safari-webdriver` or `safari-ax`; equal to `payload.engine`. |
| `capture.engine_version` | The browser version, as the browser reports it. |
| `capture.tool_version` | The version of the tool that drove the browser, for example Playwright. |
| `capture.viewport` | `{ width, height, device_pixel_ratio }`, in CSS pixels; the ratio is a float. |
| `capture.color_scheme` | `light`, `dark` or `no-preference`. |
| `capture.reduced_motion` | `reduce` or `no-preference`. |
| `capture.url` | The URL the page was loaded from. |
| `capture.settle` | The name of the settle strategy; the capture backends define the values. |
| `volatile` | `{ captured_at }`, an RFC 3339 UTC timestamp. Excluded from golden comparisons. |
| `sources[]` | `{ id, path, sha256, encoding }`: a relative `/`-separated path, 64 lowercase hex characters, and `"utf-8"`. |
| `spans[]` | `{ id, source, start_tag, element?, line, column }`: where a tagged start tag sits in a source file. |
| `diagnostics[]` | `{ code, message, node? }`: a problem capture recorded; `node` is a DOM node `id`. |
| `payload` | The engine-specific payload, tagged by `engine`. |

Fields marked `?` are optional. An optional field is omitted when it has no value, never written
as `null`.

## Units of a span

- `start_tag` and `element` are `{ start, end }` ranges of **UTF-8 byte offsets** from the start of
  the source file, **half-open**: `start` is the first byte, `end` is one past the last.
  `start_tag` runs from the `<` to just past the `>`; `element`, when the element has an end tag,
  runs to the end of it.
- `line` is **1-based**.
- `column` is **1-based** and counts **Unicode scalar values** from the start of the line, so it
  matches what an editor shows. Byte precision is in the ranges.

For example, in `<input> → <button>` at the start of a line, the button's column is 11 while its
byte offset within the line is 12, because `→` is one scalar value and three bytes.

## Chromium payload

| Field | Contents |
|---|---|
| `dom[]` | Every DOM node, flat, the document first. |
| `dom[].id` | Snapshot-local id that other fields refer to. |
| `dom[].backend_id` | The protocol's `backendNodeId`. |
| `dom[].parent` | The parent's `id`; absent only for the root. A shadow root's parent is its host. |
| `dom[].children` | Child `id`s in DOM order. A host lists its shadow root first. |
| `dom[].kind` | `document`, `doctype`, `element`, `text`, `comment`, `cdata-section`, `processing-instruction` or `document-fragment`. |
| `dom[].namespace`, `dom[].tag` | An element's namespace URI and tag name. |
| `dom[].attributes` | `{ name, value }` in DOM order, with `data-spokenpath-src` removed. |
| `dom[].text` | `{ value, truncated }` for a text-bearing node. |
| `dom[].shadow` | `open`, `closed` or `user-agent`, on a shadow root. |
| `dom[].span` | The `id` of the span locating the element's start tag. |
| `dom[].origin` | On elements: `source`, `implied`, `script`, `library` or `unknown`. |
| `ax[]` | The `Accessibility.getFullAXTree` nodes, in the protocol's own `camelCase` names. |
| `ax_forced[]` | `{ node, backend_id, nodes }`: the `getPartialAXTree` node list for each DOM element missing from the full tree, kept apart from `ax`. |
| `dom_snapshot` | Optional: the raw `DOMSnapshot.captureSnapshot` response, kept as delivered. |
| `focus` | `{ steps, truncated, cycle_start? }`: the element focused after each Tab press (`{}` when focus left the document), whether the walk stopped at its step limit, and the index of the first step that repeats an earlier step's element. |

The accessibility nodes are typed for the fields the core reads: `nodeId`, `ignored`,
`ignoredReasons`, `role`, `name`, `description`, `value`, `properties`, `parentId`, `childIds` and
`backendDOMNodeId`, with values typed as `{ type, value?, relatedNodes?, sources? }`. Name
provenance (`name.sources`) and exclusion reasons (`ignoredReasons`) are optional, because not
every engine provides them.

The `safari-webdriver` and `safari-ax` payloads have no fields yet; their contents are added in
later minor versions.

## Version rule

- A reader **rejects** a snapshot whose major version it does not know.
- A reader **accepts** any minor version, because a minor version only adds fields.
- A reader **ignores** fields it does not know, except in the accessibility nodes, where every
  protocol field the types do not name is **kept** and written back unchanged.

The Rust reader is `spokenpath_snapshot::read`. Its errors name the failing field by path, such as
`payload.dom[2].kind`.

## Golden comparisons

`volatile` differs on every run of the same capture, so golden comparisons exclude it
([ADR-0007](decisions/ADR-0007-golden-test-strategy-and-drift-reports.md)). Everything else,
including the browser version in `capture.engine_version`, is compared.

## Changing the format

1. Change the Rust types in `crates/spokenpath_snapshot`.
2. Run `cargo xtask schema` to regenerate `schema/snapshot.v0.json`.
3. Run `pnpm --filter @spokenpath/snapshot-types generate` to regenerate the TypeScript types.
4. If the typed sample changed, refresh its fixture with
   `UPDATE_FIXTURES=1 pnpm --filter @spokenpath/snapshot-types test`.

CI checks both generated files: `cargo xtask schema --check` fails when the committed schema
differs from the Rust types, and a package test fails when the committed TypeScript types differ
from the committed schema. A change to the contract also needs an ADR
([docs/decisions/README.md](decisions/README.md)).

## Fixtures

`fixtures/snapshots/` holds valid snapshots. `chromium-basic.json` is hand-written;
`chromium-from-typescript.json` is the JSON of the typed sample in `@spokenpath/snapshot-types`.
The Rust tests read and round-trip every file there, and the package tests validate every file
there.
