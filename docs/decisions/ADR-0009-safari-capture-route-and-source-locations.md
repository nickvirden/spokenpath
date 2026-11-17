# ADR-0009: Safari capture route and how source locations travel through Safari

## Status

Accepted

## Date

2026-11-17

## Context

[ADR-0006](ADR-0006-engine-scope.md) puts Safari on macOS in scope in two steps: a WebDriver
cross-check of computed role and label per element, then a native helper that reads Safari's macOS
accessibility (AX) tree. Research left open whether Safari answers both WebDriver calls for common
elements, whether the AX web tree needs a trigger before it is populated, what the helper reads
without the Accessibility permission, whether Playwright's WebKit build answers the inspector's
accessibility call, and whether a hosted macOS runner can do any of this
([Safari and VoiceOver](../research/safari-and-voiceover.md), "Not verified" and stage 0 of
"Recommended staging").

Source locations travel inside the DOM as the spliced attribute `data-spokenpath-src`
([ADR-0003](ADR-0003-source-tagging-by-splicing.md)). Chromium's protocol maps every node back to
its DOM element, so the attribute is read straight from the DOM. Safari's AX tree has no DOM node
ids, so a Safari AX node needs some other carrier that leads back to the tagged element.

Three probes answer these on macOS 27.0.1 (26A434) with Safari 27.0.1 (22625.1.29.11.28), on a
fixture of 24 tagged elements served from `127.0.0.1`:

- [Safari over WebDriver](../../spikes/safari-webdriver/README.md) calls Get Computed Role and Get
  Computed Label per tagged element.
- [Safari's AX tree](../../spikes/safari-ax-tree/README.md) drives Safari through a `safaridriver`
  session and reads the web area's AX tree from a Swift program, for three fixture variants that
  each test one carrier.
- [Playwright WebKit](../../spikes/playwright-webkit/README.md) calls
  `DOM.getAccessibilityPropertiesForNode` on Playwright 1.65.1's WebKit 27.2 build.

| Question | Result | Raw output |
|---|---|---|
| A `safaridriver` session starts and answers computed role and label per element | pass: 23 of 24 elements answered both calls; the session was deleted and `safaridriver` stopped | [`webdriver.json`](../../spikes/safari-webdriver/output/webdriver.json) |
| Element types that error or return empty over WebDriver | `optgroup` errors (`unknown error`); `img` with `usemap` and `label` return an empty role; an `aria-hidden` element answers with no sign that it is hidden | [`webdriver.json`](../../spikes/safari-webdriver/output/webdriver.json) |
| WebDriver call time | pass: 1.8 to 31.2 ms per call, 1260.8 ms for the whole run | [`webdriver.json`](../../spikes/safari-webdriver/output/webdriver.json) |
| The AX web tree is populated on the first query, without a trigger | pass: 72 nodes on the first query in every variant; no attribute was set | [`ax-tree-plain.json`](../../spikes/safari-ax-tree/output/ax-tree-plain.json) |
| AX nodes report `AXDOMIdentifier` and `AXDOMClassList` | pass: every node reports both | [`ax-tree-plain.json`](../../spikes/safari-ax-tree/output/ax-tree-plain.json) |
| Carrier: class token through `AXDOMClassList` | pass: 22 of 24 tagged elements map to exactly one node | [`carriers.json`](../../spikes/safari-ax-tree/output/carriers.json) |
| Carrier: element `id` through `AXDOMIdentifier` | fail: 21 of 24; an author `id` displaces it | [`carriers.json`](../../spikes/safari-ax-tree/output/carriers.json) |
| Carrier: document position against the WebDriver labels | fail: 16 of 24; elements with an empty label cannot be matched | [`carriers.json`](../../spikes/safari-ax-tree/output/carriers.json) |
| Any AX attribute exposes `data-*` attributes directly | fail: no attribute of any node contains a `data-spokenpath-src` value | [`carriers.json`](../../spikes/safari-ax-tree/output/carriers.json) |
| What the helper reads without the Accessibility permission | pass: run as a transient launchd job outside the terminal's permission, it reads `AXIsProcessTrusted()` false and the first call fails with `-25211` (`kAXErrorAPIDisabled`): no web area, 0 nodes, exit status 3 | [`ax-tree-untrusted.json`](../../spikes/safari-ax-tree/output/ax-tree-untrusted.json) |
| Playwright WebKit answers `DOM.getAccessibilityPropertiesForNode` | pass: 9 of 9 element nodes, reached through unsupported Playwright internals | [`accessibility-properties.json`](../../spikes/playwright-webkit/output/accessibility-properties.json) |
| A hosted macOS runner drives Safari and reads its AX tree | see "Hosted runner result" | [workflow](../../.github/workflows/macos-probes.yml) |

The two class-token misses are the `optgroup` inside a closed `select` and the
`aria-hidden="true"` `div`; neither has an AX node in any variant, so every tagged element that has
an AX node maps. The `id` carrier misses the same two plus the `input` whose author `id` wins over
the spliced one. Every one of the 16 positional matches lands on the node that carries the same
element's class token.

### Hosted runner result

The rule that depends on it: a macOS CI job is added to the checks later only if this hosted-runner
probe passed (Decision 6). The run records the image label, the macOS and Safari versions, the
WebDriver result, the AX tree result and the permission state.

PENDING: read from the macos-probes workflow run on this pull request

## Decision

1. **WebDriver cross-check.** Safari's computed role and label come from a `safaridriver`
   session, one call each per tagged element, as ADR-0006 orders. An element whose call errors is
   recorded with the raw error, not dropped. WebDriver answers do not say whether an element is
   hidden, so the cross-check is compared only against elements both engines expose.
2. **AX tree backend.** The Safari tree backend is a native helper that reads the `AXWebArea` of
   the Safari window showing the page, found by its `AXURL`, and records every node's role,
   subrole, role description, title, description, value, help, `AXDOMIdentifier` and
   `AXDOMClassList`. It needs no trigger: with the permission granted, the first query returns the
   populated tree. The helper records `AXIsProcessTrusted()` and the AX error code of every query,
   and treats `AXIsProcessTrusted()` being false or a `-25211` (`kAXErrorAPIDisabled`) error as
   "no permission": it stops, records why, and exits with a distinct status. It never infers the
   missing permission from an empty node count alone, because an empty tree can also mean the page
   is not showing; capture still checks the node count, as ADR-0006 requires. Every AX read has a
   deadline, because one probe run hung reading the web area.
3. **Source locations travel through Safari as a class token.** The one chosen carrier is a class
   token derived one-to-one from the element's `data-spokenpath-src` value (the probe used
   `sp-src-<line>-<column>`), read back through `AXDOMClassList`. On the fixture it maps 22 of 24
   tagged elements to exactly one AX node: every tagged element that has an AX node. Instrumentation
   splices the token at the same start tags as `data-spokenpath-src`, adding it to the element's
   existing `class` value when there is one, because the HTML parser drops a second `class`
   attribute.
4. **`data-spokenpath-src` stays the source attribute.** ADR-0003 is unchanged and not superseded.
   Every engine's capture reads `data-spokenpath-src` from the DOM where it can; the class token is
   an additional, Safari-specific carrier that exists only in the page served to Safari's AX
   helper, and it always agrees with the attribute it is derived from.
5. **Playwright WebKit is not a Safari capture route.** Its build answers
   `DOM.getAccessibilityPropertiesForNode`, but only through unsupported Playwright internals, it
   is a different WebKit build from the Safari it would stand in for (WebKit 27.2 against Safari
   27.0.1), and it exposes neither name provenance nor the macOS AX tree. ADR-0006's context says
   this build has no accessibility code; the probe shows the inspector call does answer. The
   decision in ADR-0006 not to use it as a stand-in for Safari stands on the reasons given here.
6. **macOS in CI.** A macOS CI job is added to the checks later only if the hosted-runner probe
   passed: `safaridriver` started a session on the hosted image and the AX probe read a populated
   tree. Until then, Safari capture runs on a developer Mac, and the
   [`macos-probes`](../../.github/workflows/macos-probes.yml) workflow stays an unrequired probe
   that `ci-ok` does not need.

## Consequences

- **DOM equality.** ADR-0003 proves instrumentation by comparing the original and instrumented
  DOMs apart from the added attribute. The page served to Safari's AX helper also carries the class
  token, so its comparison ignores exactly that one token in each tagged element's class list, and
  nothing else. Selectors that match a whole `class` value (`[class="note"]`) and scripts that read
  `className` see the extra token on that page; the `sp-src-` prefix keeps it from colliding with
  author classes. Chromium capture and the WebDriver cross-check load the page without the token.
- The token encoding has to be a valid class token, without whitespace, and reversible to the
  `data-spokenpath-src` value; the instrumentation package owns it alongside the attribute.
- Elements with no AX node (hidden content, options inside a closed `select`) have no Safari
  mapping, which is correct: Safari exposes nothing for them.
- The `id` carrier is not used, because author `id`s win over it and replacing them would break
  `for`, `aria-labelledby` and fragment references.
- Positional matching is not used as a carrier, but it is a cross-check: it agreed with the class
  token on every element it could match.
- WebDriver roles differ from Chromium's for common elements (`select` reports `button`, a `ul`
  with `list-style: none` reports `group`, `img` with `usemap` reports an empty role). The
  cross-check reports these as Safari differences; it does not correct them.
- The behaviour without the Accessibility permission is answered locally; the hosted run confirms
  it on a hosted image, whose permission state is recorded with its result.
- These answers belong to Safari 27.0.1 on macOS 27.0.1. A Safari or macOS upgrade re-runs the
  probes, and a changed answer is reviewed as drift
  ([ADR-0007](ADR-0007-golden-test-strategy-and-drift-reports.md)).

## Rejected alternatives

- **The element `id` as the carrier.** It maps 21 of 24, misses any element with an author `id`,
  and changes what `getElementById`, `#id` selectors and label references see.
- **Matching AX nodes by position against the WebDriver label sequence.** It maps 16 of 24,
  because elements without a name have nothing to match on, and it needs a WebDriver session
  alongside every AX read.
- **Reading `data-spokenpath-src` from the AX tree directly.** No AX attribute exposes `data-*`
  values.
- **Setting `AXEnhancedUserInterface` or `AXManualAccessibility` before every read.** The tree is
  populated without it, and the helper would be writing to Safari's process state for no gain.
- **Playwright WebKit's inspector call as the Safari backend.** It depends on unsupported
  internals of a test tool and on a WebKit build that is not the shipping Safari.
