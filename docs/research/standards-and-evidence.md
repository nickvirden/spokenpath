# Standards and evidence: what can honestly be claimed about screen reader output

Research date: 2026-11-16. Method: about 25 web searches and page fetches by a research agent.
Some pages and PDFs would not render; findings that rest only on search-result summaries are marked
"check before citing".

Flags: **[V]** a fetched page or search result states it; **[I]** inferred by the researcher;
**[U]** unverified. Re-verify [I] and [U] items before stating them as fact anywhere public.

## Bottom line

**What can be modelled faithfully.** The accessibility tree, the accessible name and description
computation, role, state and property exposure, and visibility, `aria-hidden` and focusability.
These are standards-defined and have conformance suites.

**What cannot be promised.** What a particular screen reader says. The specifications stop at the
accessibility tree and the platform API mapping. Wording, the order of role, name and state,
verbosity, browse versus focus mode and live-region timing are vendor-defined and vary by assistive
technology, browser, operating system, version and user settings.

**The defensible claim.** Output in two parts: a grounded record of what is exposed (role, name,
state, position, description), and a clearly labelled illustrative rendering, with per-screen-reader
phrasing only where there is recorded evidence. Claim "what the browser exposes and what a
conforming assistive technology would need to convey", not "what NVDA or VoiceOver says". The W3C
ARIA-AT project frames expected behaviour the same way.

## 1. Standards foundation (evidence: strong on scope)

- **accname 1.2** is a Working Draft. It computes a flat name or description string and leaves the
  ordering and wording of role descriptions relative to names to platform APIs or assistive
  technology. Hidden means `display:none`, `visibility:hidden` or `collapse`,
  `content-visibility:hidden`, or `aria-hidden=true`; `opacity:0` and off-screen positioning are not
  hidden; hidden content still contributes when referenced by `aria-labelledby` or
  `aria-describedby`; `::before`, `::after` and `::marker` text is concatenated [V].
  https://www.w3.org/TR/accname-1.2/
- **WAI-ARIA 1.3** is a Working Draft. It sets no speech output requirements [V; only part of the
  document was read]. https://www.w3.org/TR/wai-aria-1.3/
- **HTML-AAM** is a Working Draft. It maps HTML to MSAA/IA2, UIA, ATK/AT-SPI and AXAPI and says
  nothing about speech [V]. https://www.w3.org/TR/html-aam-1.0/
- **CSS `reading-flow`** ships in Chrome 137 and changes focus order and the order exposed to
  assistive technology for flex, grid and block containers [V].
  https://developer.chrome.com/blog/reading-flow . Support in other browsers was not found [U].
- Inferred, not fetched: Core-AAM defines API semantics; `inert` removes content from tab order and
  the tree; shadow DOM is flattened for tree computation and ID references do not cross shadow
  boundaries; CSS `order` and grid placement decouple visual order from DOM order [I].

Implication: spec-defined behaviour is a good test target, with the web-platform-tests accname and
ARIA suites as the oracle [I]. Note that every one of these documents is a draft, so any
implementation should record which draft it follows.

## 2. Screen reader divergence (evidence: moderate qualitative, weak quantitative)

- The ARIA-AT wiki gives an example of two readers announcing the same tree view differently, and
  states that agreed minimum expectations do not yet exist [V].
  https://github.com/w3c/aria-at/wiki/How-Gaps-in-Assistive-Technology-Interoperability-Hinder-Inclusion
- ARIA-AT assertions describe information conveyed (for example "role 'slider' is conveyed") with
  priorities MUST, SHOULD, MAY. There is no exact-speech field [V].
  https://github.com/w3c/aria-at/wiki/Test-Format-Definition-V2
- WebAIM: different screen reader and browser pairings behave differently for CSS; VoiceOver has no
  forms or application mode [V]. https://webaim.org/blog/screen-readers-and-css/
- WebAIM survey 10: 71.6% of respondents use more than one desktop screen reader; top pairings
  JAWS with Chrome 24.7% and NVDA with Chrome 21.3% [V].
- **Not found:** any peer-reviewed controlled study quantifying announcement divergence on identical
  markup; TalkBack or Narrator studies; a11ysupport.io and PowerMapper data could not be loaded.

A common-denominator model is defensible only at the "information conveyed" level: role, name,
state and position are generally expected. Wording, order and verbosity are not [I].

## 3. How screen reader users navigate (evidence: strong for surveys, moderate for HCI)

WebAIM survey 10 (self-report, non-random sample, 1,539 responses) [V]:
https://webaim.org/projects/screenreadersurvey10/

- 71.6% navigate long pages by headings first (78% of advanced users, 47% of beginners).
- 88.8% find heading levels useful.
- Find or search comes first for 13.6%; landmarks first for 3.7%, though 31.8% use them often or
  always; reading through first 6.4%; links first 4.8%.
- 85.9% say better websites would help more than better assistive technology.

Lazar et al. (2007), "What Frustrates Screen Reader Users on the Web", IJHCI 22(3), DOI
10.1080/10447310709336964: 100 blind users kept time diaries; top causes were confusing layout,
screen reader and application conflicts, unlabeled forms and missing alt text [search summary,
check before citing].

Implication [I]: a navigation model should cover several strategies (heading jumps, landmark jumps,
find, link lists, linear reading), not only linear reading, and express cost as steps or ranges,
not seconds.

## 4. Limits of automated testing (evidence: contested headline number)

- Deque (vendor, sells axe): 57.38% of issues by volume across its own audits [V].
  https://www.deque.com/automated-accessibility-coverage-report/ . The sample and the tool are
  Deque's own, and the metric favours high-frequency issues such as contrast.
- Vigo, Brown, Conway (W4A 2013): six tools covered 23-50% of success criteria [third-party
  summary, check before citing].
- Pool (W4A 2023, arXiv 2304.07591): nine tools on 121 pages; every tool found instances the others
  missed [V]. https://arxiv.org/pdf/2304.07591
- UK GDS tool audit: best tool caught about 40% of 142 barriers on one page [secondhand, not
  verified against the original]. https://github.com/alphagov/accessibility-tool-audit
- The 57% and 29-40% figures measure different things and are not comparable. Neither measures
  screen reader experience quality.

Detectability, by what is needed [I]:

- **Static source:** missing names, invalid role and attribute combinations, labelling relations,
  `aria-hidden` on focusable elements, heading level skips, duplicate IDs.
- **Rendered DOM and computed styles:** name computation with CSS, tree inclusion, contrast,
  visibility, focusability, DOM versus visual order, shadow DOM.
- **Interaction:** focus management, live regions, modal trapping, keyboard operability.
- **Human judgment:** alt text quality, link purpose, heading meaning, cognitive load.

## 5. Prior art on simulation (evidence: thin on fidelity)

- Guidepup drives real VoiceOver and NVDA and ships a JavaScript virtual screen reader [V].
- aDesigner "reaching time": time from page top to a target; its source notes it omits cognitive
  time [search summary]. https://doi.org/10.1007/978-3-319-67744-6_10
- WebTrax (Bigham and Murray): visualizes blind users' paths for developers [venue unconfirmed].
  https://www.cs.rochester.edu/hci/pubs/pdfs/webtrax.pdf
- Browser DevTools: Chrome source order viewer, Firefox tabbing-order overlay.
- **Not found:** any published fidelity evaluation of a simulator against recorded real speech, or
  any study showing simulation leads developers to wrong fixes. "Simulation misleads" is plausible
  and inferred, not demonstrated [I].

## 6. Developer behaviour (evidence: moderate, mostly mobile and survey-based)

- A 2026 survey of 110 mobile developers: manual checks 43%, automated tools 23%, assistive
  technologies 13%; barriers were time, stakeholder interest, budget, framework limits and
  documentation [search summary, check before citing]. https://arxiv.org/pdf/2601.14131
- No rigorous controlled study was found showing that linting, IDE feedback or visualization
  measurably improves web accessibility outcomes.

## 7. Automated repair (evidence: moderate, preprints)

- Oyelayo et al. (2026), arXiv 2605.27716: LLM fixes improved compliance in 80.2% of cases, fully
  resolved fewer than 26%, and about 30% of patches introduced structural changes; no human or
  screen reader evaluation [V]. https://arxiv.org/abs/2605.27716
- AccessGuru (arXiv 2507.19549): about 77% average similarity to developer corrections [search summary].
- Suh and Tafreshipour (arXiv 2503.15885): LLMs do better on contrast and alt text than on ARIA
  [search summary].
- No study was found measuring whether fixes that pass a checker degrade real screen reader
  experience. That risk is inferred from the WebAIM ARIA correlation [I].

## 8. Visualization (evidence: weak)

WebTrax, DevTools overlays and a reading-order bookmarklet
(https://adrianroselli.com/2019/04/reading-order-bookmarklet.html). No evaluation comparing
visualization styles for developer comprehension was found.

## 9. Validation methodology

- **Tier 1, spec conformance:** web-platform-tests accname and ARIA suites [I, directories not fetched].
- **Tier 2, ARIA-AT:** public test plans and results, at assertion level.
- **Tier 3, real speech:** AT Driver defines an `interaction.capturedOutput` event carrying the
  text sent to speech [V, Editor's Draft]. https://w3c.github.io/at-driver . Pair fixtures with
  captured NVDA or VoiceOver output to build a corpus.
- **Not found:** a public corpus of real screen reader output across many pages.

## Design implications

1. Two-layer output: a grounded "what is exposed" record, and a labelled illustrative rendering.
2. Refuse to claim exact wording, order, verbosity, live-region timing, gesture or rotor behaviour,
   or that a clean report means usable.
3. Reuse before building: the browser's own tree and existing name computation over a new parser.
4. Record which draft of each specification the model follows.
5. Model several navigation strategies; show cost as steps or ranges.
6. Mark nodes whose source mapping is uncertain.
7. Fixes: prefer native HTML over ARIA; every fix is a suggestion with a rationale; no automatic
   ARIA by default; no generated alt text without human review; re-run and show a diff.
8. Validate publicly and publish failures.
9. Do not claim the tool builds empathy or improves outcomes until measured. Avoid language like
   "experience what a blind user experiences".

## Open questions

- How large is announcement divergence across readers on a representative corpus?
- Is there a stable minimum-information model that assistive technologies agree on?
- Does predicted-output feedback change developer behaviour or outcomes on the web?
- Do checker-passing LLM fixes degrade real screen reader experience?
- Is the Deque 57% figure reproducible independently?
- How do `reading-flow` and shadow DOM behave across browsers and readers?

## Not researched or not retrievable

TalkBack, Narrator and a11ysupport.io details; PowerMapper data; the CSS Display spec text; WPT
directories; the ARIA-AT reports page; Core-AAM text; a WCAG 2.2 criterion mapping.
