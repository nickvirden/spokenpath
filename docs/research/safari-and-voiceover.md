# Safari and VoiceOver: capture routes on macOS and iOS

Research date: 2026-11-16. Method: web search and page fetches by a research agent. Nothing was
run on a Mac or a CI runner, so every capability below is a documentation or issue-tracker finding,
not a tested result. Several items depend on specific macOS, Safari or Xcode versions.

Flags: **[V]** a source was fetched that states it; **[I]** inferred; **[U]** not verified.

## Why this matters

A Chromium-only transcript does not reflect most VoiceOver users, who are on Safari. WebAIM's
screen reader survey 10 reports VoiceOver as about 70.6% of mobile screen reader use [secondary].
WebKit's accessibility tree is built separately from Chromium's and uses a different role
vocabulary, so role names and some accessible names differ.

## Bottom line

- **What Safari exposes, macOS.** The most faithful route is reading the macOS Accessibility
  (AXUIElement) tree of Safari's web content from a native helper. That is the tree VoiceOver
  consumes. It needs the Accessibility permission; working on hosted CI runners is plausible but
  unproven.
- **Cheap cross-check.** Safari's WebDriver "Get Computed Role" and "Get Computed Label" return
  per-element role and name from real Safari. No tree, no reasons for exclusion.
- **What VoiceOver says, macOS.** Drive real VoiceOver with Guidepup and read its spoken-phrase
  log. Viable but fragile, especially on hosted runners.
- **iOS.** No feasible automated route to real iOS VoiceOver speech on hosted CI was found.
  VoiceOver does not run in the iOS Simulator.
- **Not feasible:** Playwright WebKit as a stand-in for Safari's tree; a full-tree dump from
  safaridriver; hosted-runner iOS VoiceOver.

## Approaches compared

| Approach | Captures | Fidelity to what VoiceOver receives | DOM mapping | Setup | CI | Platform |
|---|---|---|---|---|---|---|
| safaridriver computed role and label | Per-element role and name | Medium | By WebDriver element, exact | `safaridriver --enable` (needs sudo), Allow Remote Automation [I] | Safari and SafariDriver are preinstalled on hosted macOS images [V]; unattended enable unverified | macOS; iOS devices [I] |
| WebKit inspector `DOM.getAccessibilityPropertiesForNode` | Per node: role, label, ignored, hidden, parent and child ids, states, owned nodes, live-region fields | Medium to high; the engine's own object, not the platform tree | By DOM node id, exact | Needs a Web Inspector connection | Plausible, unverified | macOS and iOS |
| Playwright WebKit | Nothing accessibility-specific | Low | n/a | n/a | n/a | n/a |
| macOS AXUIElement helper | Full tree: role, subrole, role description, title, description, value, help, DOM identifier, DOM class list | Highest | Through `AXDOMIdentifier` / `AXDOMClassList` if they carry enough [I], else by position | Accessibility permission | Conflicting documentation, unproven | macOS only |
| Guidepup VoiceOver | Real spoken phrases and item text | Highest, for speech | None; inferred from focus order | VoiceOver, permissions, AppleScript control | Viable but fragile | macOS only |
| AT Driver / ARIA-AT harness | Spoken text through a custom voice | Same as Guidepup | None | macOS 13+, custom voice | Hostile on hosted runners | macOS only |
| iOS routes (XCUITest, Appium, Web Inspector, device clouds) | Native-context hierarchy or web context | Partial | Weak in native context | Real device or Simulator, Xcode | Hard | iOS |

### Notes

**safaridriver.** The WebKit implementation landed in 2022 [V]
(https://bugs.webkit.org/show_bug.cgi?id=212549). Bug 253500 is still open
(https://bugs.webkit.org/show_bug.cgi?id=253500). A 2023 crash on `<area>` and `<optgroup>` was
reported (https://bugs.webkit.org/show_bug.cgi?id=255169). Current pass rates were not found. "No
tree" is inferred from the W3C command set.

**WebKit inspector.** Fields verified from the protocol definition [V]:
https://raw.githubusercontent.com/WebKit/WebKit/main/Source/JavaScriptCore/inspector/protocol/DOM.json .
The call is per node; a whole page means walking the document and issuing one call per node. It
gives `ignored` with no reasons, unlike Chromium. It is not reachable through WebDriver.

**Playwright WebKit.** Its WebKit page implementation has no accessibility code [V]
(https://raw.githubusercontent.com/microsoft/playwright/main/packages/playwright-core/src/server/webkit/wkPage.ts),
and `ariaSnapshot` is Playwright's own computation [V]. Whether Playwright's WebKit build answers
the inspector accessibility call was not tested [U]; worth a probe before ruling it out as a cheap
tree source.

**macOS AX tree.** WebKit's Mac accessibility wrapper defines `AXDOMIdentifier` and
`AXDOMClassList`, and a layout test covers them [V]
(https://bugs.webkit.org/show_bug.cgi?id=120552). Whether current Safari exposes them, and whether
a `data-*` attribute can reach the helper any other way, is unverified [U]. The web tree is built
lazily when an accessibility client asks (https://trac.webkit.org/wiki/WebKitAccessibilityPerformance);
the exact trigger is unverified [I]. Rust bindings exist: `accessibility` and `accessibility-sys`
(https://github.com/eiz/accessibility), whose safe layer is described as incomplete [V].

**Permission on hosted runners.** Two sources conflict. The `xa11y` project's CI guide says a
permission-database write works on hosted macOS (https://xa11y.dev/guides/ci/). Guidepup's setup
says such updates need System Integrity Protection off (https://github.com/guidepup/setup). Test
this early. Verified from xa11y: without the permission, queries return an empty tree silently, so
a canary on node count is required.

**Guidepup.** API includes `lastSpokenPhrase`, `spokenPhraseLog`, `itemText`, `itemTextLog` [V]
(https://www.guidepup.dev/docs/api/class-voiceover). Spoken phrases are gathered by AppleScript
polling, which causes latency, races and missed phrases [V]
(https://github.com/guidepup/guidepup/issues/129). The `guidepup/setup-action` repository is
archived; use `npx @guidepup/setup` [V].

**Hosted-runner history.** GitHub's runner-images issue 11257 was opened in December 2024 for
macOS 15 AppleScript failures and closed on 2026-06-08 after the reporter saw macos-14, 15 and 26
pass; no root cause was stated and at least one project still pinned macos-14 afterwards [V]
(https://github.com/actions/runner-images/issues/11257). Plan for regression: pin an image and
smoke-test it.

**AT Driver.** The ARIA-AT automation harness lists macOS VoiceOver as supported through a macOS
AT Driver server that uses a custom automation voice [V]
(https://github.com/w3c/aria-at-automation-harness , https://github.com/bocoup/at-driver-servers).
It is not a hosted-runner option [I].

**iOS.** VoiceOver is not available in the iOS Simulator [V]
(https://developer.apple.com/forums/thread/83458), and a 2026 report of no speech under a newer
Xcode is unresolved (https://developer.apple.com/forums/thread/831640). Appium says the XCUITest
native context exposes web content only partially [V]
(https://appium.github.io/appium-xcuitest-driver/11.0/troubleshooting/element-lookup/). Device
clouds document spoken-output capture for native apps; Safari web content was not confirmed [U].
No authoritative description of how the iOS tree for web content differs from macOS was found.

## Recommended staging

| Stage | Work | Acceptance check |
|---|---|---|
| 0 | Feasibility probes on a developer Mac and a hosted macOS runner: safaridriver role and label on a fixture; reading Safari's AX tree with the permission granted and what the DOM identifier and class list carry; whether Playwright WebKit answers the inspector call; a Guidepup smoke test | A pass/fail table per probe; the empty-tree canary is tested |
| 1 | Cross-check role and name on real Safari through safaridriver and diff against Chromium per element | The divergence fixtures show real differences and the transcript flags them |
| 2 | A Safari tree backend, preferring the AX helper. The snapshot schema treats provenance and exclusion reasons as optional, because WebKit does not provide them | Output labelled with the Safari and macOS build; source mapping succeeds on a stated share of tagged nodes |
| 3 | A VoiceOver with Safari speech profile on macOS: capture speech locally with a fixed key-press script, commit the transcripts, and only replay them in CI | Phrasing is shown only for captured cases; repeat-run agreement is measured |
| 4 | iOS: what Safari exposes through device Web Inspector; speech only from real-device captures | No iOS speech is labelled until a real-device capture backs it |

Stage 3 does not depend on hosted-runner VoiceOver working, which is why capture is local.

## Differences worth turning into fixtures

1. **`list-style: none` removes list semantics in Safari.** Treated as intentional by WebKit
   [secondary: https://www.scottohara.me/blog/2019/01/12/lists-and-safari.html ,
   https://benmyers.dev/blog/css-can-influence-screenreaders/ ]. WebKit bug 304137 reports no list
   announcement on iOS while macOS announces it, closed as a duplicate of 122320 [V]
   (https://bugs.webkit.org/show_bug.cgi?id=304137). Behaviour may differ by version; test macOS
   and iOS separately.
2. **`aria-description` together with `aria-describedby`.** WebKit bug 262751 [search summary].
3. **`aria-owns` in name computation.** WebKit bug 310639 [search summary].
   https://bugs.webkit.org/show_bug.cgi?id=310639
4. **`aria-labelledby` not re-resolved when the referenced element's name comes from `aria-label`.**
   WebKit bug 317597 [search summary]. https://bugs.webkit.org/show_bug.cgi?id=317597
5. **Inline-element spacing in name computation** differed from other engines [search summary].
6. **`display: contents`.** A web-platform-test exists; no Safari result was found. Discover by capture.
7. **Layout-table heuristics.** WebKit bug 122320 [not fetched].
8. **Live regions, `inert`, dialogs, generated content, rotor and grouping.** No sources found.
   Candidate fixtures.

## Not verified

- Whether `safaridriver --enable` and remote automation work unattended on current hosted runners.
- Current Safari pass rates for computed role and label.
- Whether Playwright's WebKit build answers the inspector accessibility call.
- Whether Accessibility Inspector shows web content from Simulator Safari.
- How the iOS tree for web content differs from macOS beyond the list case.
- Guidepup's reliability on current hosted macOS images.
- Device-cloud spoken-output capture for Safari web content on iOS.
- Whether the AX web tree needs a trigger beyond a client query, and which hosted-runner permission
  grant works without disabling System Integrity Protection.

## Sources

- https://github.com/actions/runner-images/issues/11257
- https://github.com/guidepup/setup , https://github.com/guidepup/setup-action
- https://github.com/guidepup/guidepup/issues/129 , https://www.guidepup.dev/docs/api/class-voiceover
- https://github.com/w3c/aria-at-automation-harness , https://github.com/bocoup/at-driver-servers
- https://raw.githubusercontent.com/WebKit/WebKit/main/Source/JavaScriptCore/inspector/protocol/DOM.json
- https://bugs.webkit.org/show_bug.cgi?id=253500 , https://bugs.webkit.org/show_bug.cgi?id=304137
- https://xa11y.dev/guides/ci/
- https://developer.apple.com/forums/thread/831640
- https://playwright.dev/docs/aria-snapshots
- https://github.com/eiz/accessibility

## Who uses VoiceOver with which browser (WebAIM survey 10, read directly 2026-11-16)

Source: https://webaim.org/projects/screenreadersurvey10/ (self-selected sample, 1,539 responses). Verified.

- Desktop pairings: "VoiceOver with Safari" 107 respondents (7.0%); "VoiceOver with Chrome" 30 (2.0%).
  Of those two listed pairings, about 78% are Safari. Other VoiceOver pairings are folded into
  "Other combinations" (154, 10.2%), so 78% is of the listed pairings, not of all VoiceOver users.
- Mobile: 70.6% of respondents use an Apple device; 70.6% use VoiceOver; 58.2% name Safari as
  their mobile browser.
- Supported wording: "most desktop VoiceOver users in the survey pair it with Safari". Not
  supported: "overwhelmingly".
