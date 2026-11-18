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
| `sources[]` | `{ id, path, sha256, encoding }`: a path relative to the capture root, 64 lowercase hex characters, and `"utf-8"`. The path is `/`-separated segments, none of them empty, `.` or `..`, with no leading `/` and no `\`, `:` or NUL, so joining it onto the capture root never leaves the root. |
| `spans[]` | `{ id, source, start_tag, element?, line, column }`: where a tagged start tag sits in a source file. |
| `diagnostics[]` | `{ code, message, node? }`: a problem capture recorded; `node` is a DOM node `id`. |
| `payload` | The engine-specific payload, tagged by `engine`. |

Fields marked `?` are optional. An optional field is omitted when it has no value, never written
as `null`.

## Sensitive content

A snapshot is a raw capture of the page. `capture.url` (its query string and any userinfo),
attribute values and text can hold secrets or personal data. Capture only pages you control, such
as fixtures, and review a snapshot for tokens before committing it as a golden.

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
| `focus` | `{ steps, truncated, cycle_start? }`: the DOM node of the focused accessibility node after each Tab press (`{}` when focus left the document), whether the walk stopped at its step limit, and the index of the first step that repeats an earlier step's node. |

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

Both readers also reject a snapshot whose `capture.engine` differs from `payload.engine`, and a
`sources[].path` that breaks the rule above.

The Rust reader is `spokenpath_snapshot::read`. Its errors name the failing field by path, such as
`payload.dom[2].kind`. The TypeScript `validateSnapshot` applies the same engine check, reported at
`/payload/engine`.

The two differ on purpose. `validateSnapshot` is the strict gate for writers: it enforces every
pattern and range in the schema. The Rust reader is lenient about what does not change the
meaning of a snapshot: it accepts `null` for an optional field (as absent), a `line` or `column`
of 0, and a `sha256` or `captured_at` that does not match its pattern.

## Chromium capture

`@spokenpath/capture-chromium` writes Chromium snapshots. It drives the Chromium build that its
exactly pinned playwright-core names, in new headless mode, and records what the browser exposes
as the DevTools Protocol delivers it ([ADR-0008](decisions/ADR-0008-chromium-protocol-behaviours.md)).

```sh
node packages/capture-chromium/src/cli.ts <page.html> [--out <file>] [--wait-for <selector>]
node packages/capture-chromium/src/cli.ts <page.html> --verify-instrumentation
```

The snapshot goes to stdout, or to `--out`. The command exits 0 on success, 1 when capture or
verification fails (with the reason on stderr, prefixed by its code, such as `attribute-present`),
and 2 on a usage error. The package also exports `captureChromium(pagePath, { waitFor,
timeoutMs })`, `verifyInstrumentation(pagePath, options)` and `normaliseForGolden(snapshot)`.

**Loading.** The page is tagged with `@spokenpath/instrument-html` and served at
`http://spokenpath.test/<file name>` by request interception, so capture opens no listening
socket. Other requests on that origin are answered from the page's directory and the directories
below it, except paths with a segment starting with `.` (such as `.env` or `.git/`), which get a
404 like a missing file. Requests to any other origin are aborted and every WebSocket is closed.
The browser
context is fixed: a 1280 × 720 viewport at device pixel ratio 1, light colour scheme, no reduced
motion, `en-US`, UTC, service workers blocked and downloads refused.

**Settling.** Capture waits for the `load` event, then the `--wait-for` selector when one is
given, then `document.fonts.ready`, then two animation frames. `capture.settle` is
`load-fonts-frames`, or `load-selector-fonts-frames` with a selector. Each wait, and each later
call into the browser, Tab presses of the focus walk included, has the same time limit (`timeoutMs`, a positive number of milliseconds,
30 000 by default), so a page that keeps its main thread busy fails the capture with `timeout`
rather than hanging it. A selector that never matches fails the capture, naming the selector.

**Protocol data.** After settling, capture calls `DOM.getDocument({ depth: -1, pierce: true })` and
`Accessibility.getFullAXTree` once each, `Accessibility.getPartialAXTree` (without relatives) for
every element the full tree leaves out, and `DOMSnapshot.captureSnapshot` with the computed
`display` and `visibility`. `ax`, `ax_forced` and `dom_snapshot` hold the responses unchanged.
`dom` is the document tree flattened in pre-order (the document is id 0), with these limits of
what the protocol delivers:

- `DOM.getDocument` leaves out text nodes that hold only whitespace, so `dom` has none.
- `namespace` is never written, because the protocol does not deliver it.
- Pseudo-elements are not recorded; the snapshot has no kind for them.
- A `template`'s content fragment is recorded without children, as the protocol returns it.
- Frame contents are not followed (see `iframe-not-captured`).
- Text is never shortened, so `text.truncated` is always `false`.

**Origins.** Each element's `origin` comes from the first rule that matches:

1. `source`, with `span`, when its `data-spokenpath-src` value is the id of a span;
2. `unknown` when that attribute is present but names no span;
3. `unknown` inside a user-agent shadow root, which the browser builds itself;
4. `implied` for an element in the document's light tree when the HTML parser, run on the served
   text, creates an element of the same name at the same place without a start tag (implied
   `html`, `head`, `body` or `tbody`);
5. `script` otherwise, such as content a script adds or attaches in a shadow root.

`library` is not written by this capture.

**Focus walk.** After the protocol data, capture presses Tab and, after each press, reads
`Accessibility.getFullAXTree` again and records the DOM node of the node marked `focused`, leaving
out the root, which is marked whenever the document has focus. That node tells apart stops that
`document.activeElement` cannot: each field and the calendar button of a date input are nodes in
its user-agent shadow root, and each control in a closed shadow root is its own node, while the
active element stays on the input or the host. A step is `{}` when the document does not have
focus or only `body` is focused. Chromium moves focus out of the document after the last stop and
back to the first stop on the next press, so the walk continues past the first `{}`. It stops at
the first of:

- a step on a node an earlier step already recorded, which it keeps, with `cycle_start` set to
  that step's index; a trap shows a cycle with no `{}` before it, and a script that holds focus
  on one element shows the cycle at the step right after it;
- the second `{}` step;
- 200 steps, with `truncated: true`.

When the tree has no focused node, the step falls back to the active element, descending through
open shadow roots. That happens when focus is inside a frame, whose tree capture does not read, or
on an element with no accessibility node. Inside a frame the step is the `iframe` or `frame`
element, which stays the same while focus moves between the frame's own stops. A step that
repeats the frame element of the step just before it is therefore the one exception to the first
rule: the walk records one step per press and keeps going, and the frame's `iframe-not-captured`
diagnostic marks the case.

**Diagnostics.** Sorted by code, then node, then message. URLs in messages have their userinfo,
query string and fragment removed.

| Code | Meaning |
|---|---|
| `external-request-blocked` | A request to another origin was aborted, or a WebSocket was closed. |
| `asset-not-found` | A request on the page's origin named no file in its directory, or one outside it; it got a 404. |
| `iframe-not-captured` | An `iframe`, `frame` or other element with a frame document; the element is recorded, its contents are not. |
| `unknown-source-attribute` | An element's `data-spokenpath-src` names no span, so its origin is `unknown`. |
| `partial-ax-tree-failed` | `getPartialAXTree` failed for the element, so it has no `ax_forced` entry. |
| `focus-target-not-in-dom` | Focus landed on an element missing from `dom`, such as one a script added after the DOM was read; the step has only `backend_id`. |
| `focus-in-closed-shadow-root` | The tree had no focused node and the active element is a host whose shadow root is closed, so the element focused inside it cannot be read. |

**Verifying instrumentation.** `--verify-instrumentation` loads the original page and the
instrumented page in two separate loads, served and settled the same way, and compares the two
`DOM.getDocument` trees: node types, names, values, doctype identifiers, shadow root types,
attributes in order (leaving out `data-spokenpath-src` on the instrumented side only) and the
child structure, ignoring protocol ids. It prints one line and exits 0 when they are equal and 1
at the first difference. It writes no snapshot.

## Golden comparisons

Golden comparisons compare a normalised copy of each snapshot
([ADR-0007](decisions/ADR-0007-golden-test-strategy-and-drift-reports.md)).
`normaliseForGolden` rewrites the fields that differ between runs or hosts of the same capture:

| Field | Set to | Why it varies |
|---|---|---|
| `volatile.captured_at` | `1970-01-01T00:00:00Z` | The time of each run. |
| `producer.version` | `0.0.0` | Every release bumps it. |
| `backendNodeId`, `backend_id` and `backendDOMNodeId`, everywhere (including `relatedNodes` and the DOM snapshot's `backendNodeId` arrays) | Renumbered from 1 by first appearance in `dom`, then `ax`, `ax_forced`, `focus` and `dom_snapshot`, the same number everywhere | The browser assigns them at run time. |
| AX `nodeId`, `parentId` and `childIds`, in `ax` and `ax_forced` | Renumbered from 1 by first appearance in `ax`, then `ax_forced`, the same number everywhere | Chromium assigns them from process-wide counters; the `InlineTextBox` ids differ between macOS and Linux. |
| AX `frameId`, and the DOM snapshot document's `frameId` string | `frame` | Random per browser launch. |
| `dom_snapshot` layout `bounds`, `offsetRects`, `scrollRects` and `clientRects`, text box `bounds`, and each document's `contentWidth`, `contentHeight`, `scrollOffsetX` and `scrollOffsetY` | `0`, keeping array shapes | They depend on the host's fonts. |

Everything else, including the browser version in `capture.engine_version`, is compared, so a
browser upgrade shows up as a reviewed diff. The capture goldens live in
`fixtures/snapshots/chromium/`, one per page directly in `fixtures/pages/` or in
`fixtures/pages/focus/`, at the same relative path with `.json` in place of `.html`. Refresh them
with `UPDATE_FIXTURES=1 pnpm test:capture`. Fixture text stays on one line at 1280 pixels, so
fonts cannot change how text wraps.

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

`fixtures/snapshots/` holds valid snapshots. `chromium-basic.json` is hand-written from the page
in `fixtures/snapshots/pages/basic.html`; a package test checks the source hash and slices every
span by its byte range against that page. `chromium-from-typescript.json` is the JSON of the typed
sample in `@spokenpath/snapshot-types`. The Rust tests read and round-trip every `.json` file
there, and the package tests validate every one. `fixtures/source-paths.json` lists the
accepted and rejected source paths that both readers' tests use.
