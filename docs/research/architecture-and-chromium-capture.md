# Architecture and Chromium capture

Research date: 2026-11-16. Two passes: an architecture survey (Rust distribution, building blocks,
source mapping, fix models) and an adversarial design review that checked the Chrome DevTools
Protocol definition, Chromium's Blink source, and the Playwright and parse5 documentation.

Flags: **[V]** a source was fetched that states it; **[I]** inferred; **[U]** not confirmed.
Version numbers below were read on the research date and must be re-checked at install time.

## Bottom line

Accuracy has to come from a real browser. The core should be a library that consumes a serialized
snapshot captured from the browser and does everything else: normalising, reading order,
announcement rendering, and later rules and fixes. Static analysis of HTML or JSX source can only
ever be a reduced-fidelity mode, because CSS, visibility, shadow DOM and rendered React output are
invisible to it.

Rust is defensible but its usual justification (parsing speed) does not apply here. It earns its
place through one core running in many hosts (CLI, npm native module, WASM, Python wheel), a strict
serializable data model, and batch runs in CI. That only holds if the Rust core does the
interpreting; if the capture layer shapes the data, every future capture port has to repeat that
logic.

## Three architectures compared

| | Static analysis over source | Core over a runtime snapshot | TypeScript only, in the page |
|---|---|---|---|
| Can see | Authored attributes and literal text; CSS only approximately | Everything the browser resolved | Same, plus live state |
| Accuracy ceiling | Low to medium | High | High |
| Portability | Excellent | Excellent; the snapshot is the contract | Weak outside JavaScript |
| Solo-maintainer cost | Deceptively high: it rebuilds a browser | Moderate | Lowest |

Evidence for the real-browser gap: `dom-accessibility-api` passes 153 of 159 web-platform-tests in
Chrome but 138 in jsdom, where "visibility context is missing" [V].
https://github.com/eps1lon/dom-accessibility-api

## What Chromium gives over the DevTools Protocol

Sources: the protocol definition
(https://raw.githubusercontent.com/ChromeDevTools/devtools-protocol/master/pdl/domains/Accessibility.pdl)
and Blink's inspector accessibility agent sources in the Chromium repository.

- `Accessibility.getFullAXTree` returns **name provenance** on every exposed node: the source type,
  the attribute, the attribute value, a native source (label, `labelfor`, wrapped label, legend,
  figcaption and others), and `superseded`, `invalid` and `invalidReason` flags [V]. So "why is the
  name this" is available without implementing name computation.
- **Descriptions carry no provenance.** The description string is emitted without a source list [V].
  "Described by X" has to be worked out from the raw DOM attributes.
- **Position in set is not in the protocol.** The property list has `level` but no `posinset` or
  `setsize` [V]. The core computes them and must label them as computed by the tool.
- **Ignored nodes lose their identity.** An ignored node appears with `ignored: true` and
  `ignoredReasons` (among them `ariaHiddenElement`, `ariaHiddenSubtree`, `inertElement`,
  `inertSubtree`, `notRendered`, `notVisible`, `presentationalRole`, `emptyAlt`, `labelFor`,
  `activeModalDialog`, `uninteresting`), but its role is `none` and it has no name [V]. Joining to
  the DOM gives the tag; a follow-up `getPartialAXTree` call per ignored element may recover role
  and name [I, to be confirmed by a probe].
- The whole Accessibility domain is marked **experimental** [V]. It has been stable in practice for
  years [I]. Pin the browser version anyway.
- Reachable from Playwright through `context.newCDPSession(page)`, Chromium only [V].
  https://playwright.dev/docs/api/class-browsercontext#browser-context-new-cdp-session
- `getFullAXTree` walks one frame's document; iframes need a call per frame and out-of-process
  iframes need their own sessions [partly I].
- One `DOM.getDocument({depth:-1, pierce:true})` call returns every node with its backend id,
  attributes, shadow roots and template content, so the join can be done locally [I].
- There is no protocol call for tab order. Computing it from `tabindex` is wrong once shadow
  scoping, `delegatesFocus`, `reading-flow` or `inert` are involved. Press Tab for real and resolve
  the focused element [I].

**What Layer A is and is not.** It is exactly what Chromium's internal tree (Blink) exposes for one
build. It is not the specification: where Chromium deviates, its bug is reported as fact. It is not
what assistive technology receives: screen readers read the platform trees (UIA, IA2, the macOS AX
API, ATK). Label it with the engine and version.

## Reading order

Chromium's accessibility child order follows the flat tree (shadow slotting), re-parents
`aria-owns` targets, and reflects `reading-flow` from Chrome 137 [V for reading-flow, I for the
rest]. CSS `order` and grid placement without `reading-flow` do not reorder, which matches DOM-order
reading. Caveats: `aria-flowto` is honoured by few readers; adjacent text runs should be merged.

## Source mapping

- **Plain HTML.** parse5 with `sourceCodeLocationInfo` gives start-tag offsets and gives no location
  for implied elements [V]. https://parse5.js.org/interfaces/parse5.ParserOptions.html . Its offsets
  are UTF-16 code units, so a schema must pick units explicitly.
- **Never re-serialise.** Parsing and re-serialising changes the DOM (entity normalisation, implied
  `tbody`, adoption-agency clones, `template` and `noscript` handling). Use the parser only to find
  start tags, then splice an attribute into the original text. Prove it: load the original and the
  instrumented page and assert the DOM trees are equal apart from the attribute.
- Remaining edge cases: encodings other than UTF-8, scripts that enumerate attributes, implied and
  script-created elements (classify them, do not leave them blank).
- Take any source excerpt from the original bytes by span, not from the browser's outer HTML.
- **React.** React 19 removed `__source`, `__self` and Fiber `_debugSource` [V].
  https://github.com/facebook/react/pull/28265 . The practical route is a build-time transform that
  injects a `data-*` source attribute, which then travels inside the DOM. The transform tags the
  host element where its JSX is written, often inside a design-system component, so an owner chain
  will be needed later, and prebuilt dependencies stay untagged.

## Other capture facts

- Playwright `ariaSnapshot` is Playwright's own JavaScript computation, designed to give the same
  output in every engine. It is not any engine's accessibility tree, and it has no description or
  provenance [V].
  https://raw.githubusercontent.com/microsoft/playwright/main/packages/injected/src/ariaSnapshot.ts
- `file://` behaves differently from http for module scripts and fetch. Serve pages through request
  interception and block external network by default. Captured pages run their scripts.

## The failure mode to watch: silent drift

Chromium's accessibility output changes between releases. If golden tests run against a live
browser, every dependency bump either breaks CI or gets approved without review. Split the goldens:

1. Render goldens (snapshot to transcript): deterministic, no browser.
2. Capture goldens (HTML to snapshot): tied to a pinned browser version, volatile fields normalised.
3. A capture diff on a browser bump is a reviewed drift report.

Record the browser version in every snapshot and transcript.

## Distribution patterns (for later)

- **napi-rs** v3: per-platform npm packages listed as optional dependencies; a WASI build as
  fallback [V]. https://napi.rs/docs/introduction/getting-started
- The rustwasm GitHub organization was sunset in 2025; call `wasm-bindgen` directly rather than
  relying on wasm-pack [V/I].
  https://blog.rust-lang.org/inside-rust/2025/07/21/sunsetting-the-rustwasm-github-org
- **maturin and PyO3** build Python wheels [V]. https://github.com/PyO3/maturin
- **UniFFI** targets Kotlin, Swift, Python and Ruby officially [V]. https://github.com/mozilla/uniffi-rs
- Layouts to copy: oxc (`crates/`, `napi/`, `npm/`, pnpm workspace), Biome (`crates/`, `packages/`,
  `xtask/`), Ruff (`crates/`, `python/`) [V]. Their release workflows were not inspected.

## Building blocks surveyed

| Block | Licence | Note |
|---|---|---|
| html5ever | MIT or Apache-2.0 | Spec-compliant; no convenient source spans [I] |
| lol_html | ISC | Streaming rewriter, token level; poor fit [I] |
| oxc_parser | MIT | JSX/TSX with spans; fast release cadence [V] |
| lightningcss | MPL-2.0 | Parsing only, no cascade [V] |
| stylo | MPL-2.0 | Real cascade, heavy to embed [I] |
| AccessKit | Apache-2.0 or MIT | A UI-toolkit tree schema; no name computation [V] |
| `accname` crate | MIT | Two months old, no conformance claim; vet before depending on it [V/I] |

## Fix models in comparable tools

- oxlint: safe fix, suggestion, dangerous fix [V].
  https://oxc.rs/docs/guide/usage/linter/automatic-fixes.html
- Biome: safe and unsafe [V]. https://biomejs.dev/linter/
- Ruff: safe and unsafe, with an `applicability` field on every fix in JSON output [V].
  https://docs.astral.sh/ruff/linter/

For accessibility, a third tier is needed: fixes that require human-authored content (alt text,
labels, link text) [I].

## Output formats and integrations [I]

JSON with a stable schema, SARIF, a plain-text transcript, Mermaid for navigation flow, a static
HTML report. Integration surfaces in rough priority: CLI, Playwright and Vitest matchers, GitHub
Action, MCP server, Vite plugin, Storybook addon, editor extension.

## Not confirmed

wasm-pack's current status; the web-platform-tests `wai-aria` directory; the release pipelines of
oxc, Biome and Ruff; whether a `getPartialAXTree` call forces role and name for ignored nodes;
`relatedNodes` identifying the hiding ancestor.
