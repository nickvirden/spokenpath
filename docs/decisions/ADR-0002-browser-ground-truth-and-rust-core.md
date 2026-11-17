# ADR-0002: Real browsers are ground truth; a Rust core does all interpretation

## Status

Accepted

## Date

2026-11-16

## Context

What a page exposes to assistive technology depends on CSS, visibility, shadow DOM, script and the
browser's own accessibility code. Static analysis of source cannot see most of that, and JavaScript
reimplementations of name computation lose accuracy outside a real browser: `dom-accessibility-api`
passes 153 of 159 web-platform-tests in Chrome but 138 in jsdom [V]
([architecture and Chromium capture](../research/architecture-and-chromium-capture.md)).

Chromium's DevTools Protocol returns name provenance on every exposed node, so "why is the name
this" needs no reimplementation. Its accessibility output changes between releases, so captured
data has to be re-interpretable after an adapter fix.

Rust is justified here by portability across hosts (CLI, npm native module, WASM, Python wheel),
not by speed, and only if the Rust core is the part that interprets.

## Decision

- **Real browsers are ground truth.** Role and accessible name come from the real browser's
  accessibility tree. spokenpath does not compute names itself; a Rust accessible-name
  implementation waits for a later no-browser mode, validated against the web-platform-tests
  suites and against the browser.
- **Capture never interprets.** A TypeScript capture layer dumps raw data into a versioned JSON
  snapshot: the raw DOM, the raw accessibility tree, a real Tab-key focus walk and metadata
  (engine, version, viewport). It does not join, filter or shape records.
- **The Rust core does all interpretation.** A per-engine adapter reads the snapshot into an
  engine-neutral model; the core then computes reading order and navigation indexes and renders
  the transcript.
- Node, WASM and Python bindings wait until the snapshot schema settles.

## Consequences

- The snapshot is the contract between capture and core. Its schema is versioned, and an unknown
  major version is rejected.
- Capture ports for other engines stay small, because they only dump data.
- Old snapshots can be re-interpreted after an adapter fix without re-capturing.
- Capture needs a real browser, so it runs the page's scripts (see `SECURITY.md`).
- Anything the browser does not provide (for example position in a set in Chromium's protocol)
  is computed by the core and labelled "computed by tool" ([ADR-0001](ADR-0001-two-layer-output.md)).

## Rejected alternatives

- **Static analysis of HTML or JSX source as the primary path.** CSS, visibility, shadow DOM and
  rendered React output are invisible to it, so its accuracy ceiling is low, and making it accurate
  means rebuilding a browser.
- **jsdom-based name computation.** It lacks the visibility context of a real browser and scores
  lower on the same conformance tests.
- **Depending on the `accname` Rust crate.** It is young (created 2026-09-18) and claims no
  web-platform-tests conformance, so it cannot be trusted as ground truth.
- **TypeScript only.** Fastest to a first version, but weak outside JavaScript, which removes the
  reason for a portable core.
- **Letting the capture layer join and shape records.** Every future capture port would have to
  repeat that logic, and a shaping bug would be baked into stored snapshots.
- **Shipping language bindings in the first release.** Bindings fix the API before the snapshot
  schema has settled.
