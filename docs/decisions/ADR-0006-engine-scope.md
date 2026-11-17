# ADR-0006: Engine scope: Chromium first, then Safari and VoiceOver on macOS

## Status

Accepted

## Date

2026-11-16

## Context

Chromium's DevTools Protocol gives the richest accessibility data of any engine, including name
provenance and exclusion reasons
([architecture and Chromium capture](../research/architecture-and-chromium-capture.md)). A
Chromium-only transcript, however, misses most VoiceOver users: most desktop VoiceOver users in
WebAIM's screen reader survey 10 pair it with Safari [V], and WebKit builds its accessibility tree
separately from Chromium, with a different role vocabulary
([Safari and VoiceOver](../research/safari-and-voiceover.md)).

On macOS, the tree VoiceOver consumes is Safari's macOS accessibility (AX) tree, readable from a
native helper with the Accessibility permission. Real Safari also answers WebDriver "Get Computed
Role" and "Get Computed Label" per element. Real VoiceOver speech can be captured with Guidepup,
but capture gathers phrases by polling, so it is flaky, and hosted CI runners are unreliable for
it. VoiceOver does not run in the iOS Simulator [V]. Playwright's WebKit build has no
accessibility code [V], and Playwright's `ariaSnapshot` is Playwright's own computation, not any
engine's tree [V].

## Decision

In scope, in this order:

1. **Chromium first**, captured through Playwright and the DevTools Protocol.
2. **Safari on macOS through a WebDriver cross-check**: per-element computed role and label from
   real Safari, diffed against Chromium, with a note wherever Safari differs.
3. **Safari on macOS through a native helper** that reads Safari's macOS AX tree, as a second
   capture backend.
4. **A "VoiceOver + Safari on macOS" speech profile**, built from real VoiceOver speech captured
   locally and committed as a corpus that records the macOS, Safari and VoiceOver versions. CI
   replays the committed corpus; it does not drive VoiceOver.

Out of scope: iOS; Firefox; NVDA and JAWS speech profiles; iframes; live regions and dynamic
updates; React source mapping; rule checking; fixes; language bindings.

## Consequences

- The snapshot schema treats the engine as an enum and makes provenance and exclusion reasons
  optional, because WebKit does not provide them.
- Safari capture needs a Mac with Safari remote automation enabled and, for the AX helper, the
  Accessibility permission. Without that permission the helper reads an empty tree silently, so
  capture checks node counts.
- Screen reader phrasing is shown only for patterns present in the captured corpus
  ([ADR-0001](ADR-0001-two-layer-output.md)); everything else stays neutral and says so.
- Out-of-scope items are listed as later work in [the roadmap](../roadmap.md).

## Rejected alternatives

- **Chromium only.** It misses the browser that most desktop VoiceOver users in the survey pair
  VoiceOver with.
- **Stopping at the WebDriver cross-check.** It gives role and name per element only, with no tree
  and no exclusion reasons, so it cannot back a Safari transcript.
- **Gating VoiceOver capture in CI.** Capture is flaky and hosted runners are unreliable for it,
  so CI would fail for reasons unrelated to the change under test.
- **Claiming anything about iOS speech.** VoiceOver does not run in the iOS Simulator, so iOS
  speech needs real-device captures that do not exist yet.
- **Playwright WebKit as a stand-in for Safari.** It has no accessibility code, and its
  `ariaSnapshot` is Playwright's own computation rather than WebKit's tree.
