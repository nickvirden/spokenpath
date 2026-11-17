# Market landscape: tools that predict or capture screen reader output

Research date: 2026-11-16. Method: web search and page fetches by a research agent. Star counts,
download counts and push dates came from the GitHub and npm APIs on that date.

Flags used throughout: **[V]** a source was fetched that states it; **[I]** inferred by the
researcher; **[U]** not verified, or the source is weak. Re-verify [I] and [U] items before stating
them as fact anywhere public.

## Bottom line

Two things already exist that might be assumed to be open gaps:

- **A simulated screen reader for tests.** Guidepup's `virtual-screen-reader` is MIT-licensed, had
  about 113k weekly npm downloads and was last pushed 2026-11-05 [V].
- **A "predict NVDA/JAWS/VoiceOver output from static HTML" tool.** `@reticular/speakable` (repo
  `R3TICULAR/AnnounceKit`) was first published in April 2026, is at v1.8.0, ships a CLI, an MCP
  server and a Storybook add-on, and is MIT-licensed [V]. It had 1 GitHub star [V] and publishes no
  validation against real screen readers [V].

Accessibility-tree inspection and rule engines are solved and dominated by incumbents. The gaps the
search found (the search was not exhaustive):

- **Source mapping.** No tool was found that maps each announcement back to its HTML/JSX tag [U].
- **Navigation-path artifacts.** None was found that combines prose, flow diagrams and a report [I].
- **Fixes grounded in the announcement.** The fix loop exists only as LLM remediation inside
  commercial suites [I].
- **A native, multi-language core.** Everything in the prediction space is JavaScript on jsdom [I].

Whether the project is worth building depends on earning trust on accuracy: publishing measured
agreement with real browsers and real screen readers, not claiming "faithful". Without that it is a
me-too to Speakable and Guidepup's virtual screen reader. A Rust core is a distribution advantage,
not a differentiator users ask for [I].

## Comparison table

| Tool | Category | Approach | Licence / price | Health | Key weakness |
|---|---|---|---|---|---|
| Guidepup `virtual-screen-reader` | Simulated screen reader | Walks the DOM and emits spoken-phrase logs for Jest/Playwright-style tests [V] | MIT [V] | 159 stars, 17 open issues, pushed 2026-11-05, ~113k weekly downloads, latest 0.33.0 [V]; one main author [I] | One generic output, not per reader [V]. README lists 99 failing and 347 skipped WPT tests and says it is "not a substitute" for real testing [V]. No source mapping, visualization or fixes |
| Guidepup (real automation) | Real screen reader automation | Drives VoiceOver (macOS), NVDA (Windows) and Orca (Linux) [V] | MIT [V] | 599 stars, 14 open issues, pushed 2026-11-08, ~96k weekly downloads [V] | No JAWS or TalkBack listed [V]. Needs OS-specific setup. Not a prediction tool |
| `@reticular/speakable` (AnnounceKit) | Simulated screen reader, closest competitor | jsdom, then an ARIA tree, then a deterministic model, then per-reader renderers for NVDA, JAWS, VoiceOver and Narrator; CLI, MCP, Storybook add-on [V] | MIT [V] | Created 2026-04-12, v1.8.0, 13 versions, modified 2026-11-07, 1 star [V] | Heuristic; README says it does not perfectly replicate screen readers [V]. No validation against real readers [V]. JSX source mapping not confirmed [U] |
| Silktide screen reader simulator | Simulated, visual | Chrome/Edge extension with heading, landmark and form navigation [V] | Free, closed source [V] | Store rating ~3.9, ~10k users [V] | Calls itself "an introduction, not a replacement" [V]. Not a library, no fixes |
| Assistiv Labs | Real screen readers in the cloud | Remote NVDA, JAWS, VoiceOver, TalkBack, Narrator [V] | Commercial; prices not on the page [V] | Active; automated end-to-end in private beta [V] | Vendor coverage claims are self-reported. Closed |
| BrowserStack screen reader automation | Real screen readers in the cloud | Captures screen reader output and traversal order [V, Dec 2025 post] | Commercial; prices not checked [U] | Active [V] | Supported readers and frameworks not specified in what was read [V] |
| W3C ARIA-AT / AT Driver | Standards infrastructure | Protocol to drive JAWS, NVDA and VoiceOver plus a support-test corpus [V] | W3C, open | AT Driver is a draft [V] | Not a developer tool; a source of ground truth |
| Playwright ARIA snapshots | Tree snapshot | YAML view of the accessibility tree with `toMatchAriaSnapshot` [V] | Apache-2.0 [I] | Very active [I] | A tree, not an announcement; cannot map to source |
| axe-core / Deque axe DevTools | Rule engine plus AI | Rule engine; Axe MCP Server (Feb 2026) [V] | axe-core MPL-2.0; DevTools commercial [V] | 7.8k stars, 489 open issues, ~115M weekly downloads [V] | Finds rule violations, does not narrate the experience. AI fixes are closed |
| `eslint-plugin-jsx-a11y` | Linter | Static rules on JSX | MIT [V] | 3.6k stars, 148 open issues, last push 2026-01-06, ~67M weekly downloads [V]; slow maintenance [I] | JSX only, no reading-order concept |
| oxlint (jsx-a11y rules) | Rust linter | Rust port of the ESLint rules; autofix on a few [V] | MIT [V] | oxc: 23.5k stars [V] | No HTML accessibility rules found [U]; no screen reader modelling |
| Biome HTML a11y rules | Rust linter | HTML lint rules added in 2026 [secondary] | MIT [I] | Active [I] | Rule-level only; docs page not fetched [U] |
| `axl` | Rust a11y linter | JSX/TSX rules in Rust, limited autofix [secondary] | Unknown [U] | Early-stage [secondary] | Scaffold-level |
| Stark, Evinced | Commercial AI remediation | IDE and CI fix suggestions [V, vendor material] | Commercial; prices not found [U] | Active | Closed; no independent reviews found |
| Polypane, Accessibility Insights, browser DevTools | Visualization | Tab-order overlays, live accessibility tree, outline panels [V] | Mixed | Active [I] | Live page only; no prose, source mapping or CI |
| Overlays (accessiBe, UserWay) | Overlay | Script that patches the live page | Commercial | FTC order against accessiBe, $1M, 2025 [secondary] | Anti-pattern; any "autofix" feature will be compared with them |

Not fetched: Pa11y, IBM Equal Access, WAVE, Lighthouse, Storybook a11y. `dom-accessibility-api` had
about 189M weekly downloads [V] and is the dominant accessible-name library in JavaScript.

Rust crates in the neighbourhood, all young [V from crates.io]: `accname` 0.30.7 (created
2026-09-18, 57 versions, from `casoon/barrierlab`, no WPT conformance claimed), `pageon-ax` 0.3.0
(created 2026-11-15), `a11y-static` 0.5.1, `a11y-wasm` 0.16.0.

## Gap analysis against the product idea

| Point | Status | Verdict |
|---|---|---|
| Takes HTML, then JSX, then other frameworks | Partly served | Speakable and the virtual screen reader take HTML. No Rust-native tool does [I] |
| Faithful prediction of announcements and navigation | Partly served, weakest on faithfulness | Existing simulators are explicitly heuristic. The unmet need is measured fidelity, not another simulator [I] |
| Maps announcements to the source tag | Looks unserved | No tool found doing this; search limited [U] |
| Prose, flow diagram, report | Unserved as one artifact | Pieces exist in separate tools [I] |
| Issue detection, fixes, autofix | Detection solved; autofix partly | Integrate axe-core rather than rebuild [I] |
| Rust core with JS and Python bindings | Unserved for this domain | No Rust screen reader model found [I] |
| Open source | Not a gap | Open competitors are MIT |

Boundary: integrate axe-core for rules and use `dom-accessibility-api`, the accname spec, ARIA-AT
and a11ysupport.io as oracles; do not rebuild real-screen-reader automation; build source-mapped
announcements, navigation artifacts and the core.

## Accuracy and trust risk

- Every existing simulator hedges: Guidepup "no substitute" [V], Silktide "introduction, not a
  replacement" [V], Speakable "heuristic... does not perfectly replicate" [V].
- Guidepup's README says screen reader output has no formal spec, so a11ysupport.io was used as the
  guide [V]. A "faithful prediction" claim has no authoritative target [I].
- One practitioner article reports a simulator extension flagging serious problems that VoiceOver
  did not show [secondary, original not verified].
- No statement by a named accessibility specialist on simulators was found. Expect skepticism toward
  "automated accessibility" claims [I].
- Several figures come from vendors and should be weighted accordingly.

How to hedge better than incumbents: publish measured agreement tables; label every line with its
source and confidence; frame output as "what changed" as much as "what is said".

## Demand evidence

- WebAIM Million 2026: 95.9% of home pages had detectable WCAG failures, 56.1 errors per page on
  average, 82.7% of pages used ARIA; pages with ARIA averaged 59.1 errors against 42 without, which
  is correlation, not causation [V]. https://webaim.org/projects/million/
- WebAIM Screen Reader User Survey 10 (1,539 responses, self-selected): JAWS 40.5% and NVDA 37.7%
  as primary desktop readers, VoiceOver 9.7% on desktop and about 70.6% of mobile use [V/secondary].
  https://webaim.org/projects/screenreadersurvey10/
- Seyfarth counted 3,117 federal website accessibility suits in 2025 [V via search].
- Developer-side: surveys suggest browser DevTools lead as the accessibility testing tool and
  screen readers trail [secondary, page not fetched]. No clean percentage found.
- No evidence was found of willingness to pay for a developer-facing screen reader preview.
- Legal pressure pushes organizations toward audit and compliance tools more than developer
  previewers [I].

## Positioning options

1. **Announcement diffs in CI.** Each pull request shows how exposed output changed, with source
   locations. The most defensible claim.
2. **Explain the page to a developer in prose and diagrams.** Lowest trust risk, least differentiated.
3. **The engine.** A core that linters, editors and agents embed. Slow adoption, but the
   distribution is what the market lacks [I].

Differentiation: against Speakable, verifiable accuracy and source mapping; against the virtual
screen reader, real-browser capture and source-level reports; against Deque, Evinced and Stark,
open source, determinism and local use.

## Risks

- Accuracy disputes: one public bad prediction can cost credibility with specialists.
- Maintenance: browser and screen reader behaviour moves with every release and has no spec.
- Hostility to automation claims: avoid "compliant", "accessible in one click" and overlay vocabulary.
- A well-resourced vendor could ship the same claim.
- Multi-language bindings are a real cost for a solo maintainer; the Python audience is unproven [I].

## Not found or unverified

Pricing for the commercial tools; JAWS or TalkBack coverage in any open tool; Reddit and Hacker
News threads (only secondary references); an independent accuracy evaluation of any simulator;
exhaustive confirmation that no tool does source mapping or flow diagrams.

## Sources

- https://github.com/guidepup/virtual-screen-reader , https://github.com/guidepup/guidepup
- https://registry.npmjs.org/@reticular/speakable , https://github.com/R3TICULAR/AnnounceKit
- https://silktide.com/toolbar/screen-reader-simulator/
- https://assistivlabs.com , https://assistivlabs.com/articles/automating-screen-readers-for-accessibility-testing
- https://github.com/w3c/at-driver
- https://playwright.dev/docs/aria-snapshots
- https://api.github.com/repos/dequelabs/axe-core
- https://api.github.com/repos/jsx-eslint/eslint-plugin-jsx-a11y , https://oxc.rs/docs/guide/usage/linter/rules.html
- https://github.com/jose-compu/axl
- https://polypane.app/docs/outline-panel , https://accessibilityinsights.io/docs/web/overview/
- https://adrianroselli.com/2025/01/ftc-catches-up-to-accessibe.html
- https://adrianroselli.com/2023/01/comparing-manual-and-free-automated-wcag-reviews.html
- https://webaim.org/projects/million/ , https://webaim.org/projects/screenreadersurvey10/
- npm download API: https://api.npmjs.org/downloads/point/last-week/<package>
