# spokenpath

[![CI status](https://github.com/nickvirden/spokenpath/actions/workflows/ci.yml/badge.svg)](https://github.com/nickvirden/spokenpath/actions/workflows/ci.yml)
[![Licence: MIT OR Apache-2.0](https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-0b5ea8)](#licence)
[![OpenSSF Scorecard](https://api.scorecard.dev/projects/github.com/nickvirden/spokenpath/badge)](https://scorecard.dev/viewer/?uri=github.com/nickvirden/spokenpath)

spokenpath is designed to show what a real browser exposes to screen readers on a web page, line by
line and traced back to the HTML that produced it, so you can review it without running a screen
reader.

## Status

spokenpath is **pre-alpha**. Nothing is installable yet and no package is published: the
`spokenpath` command runs from a checkout of this repository, for Chromium only
([docs/usage.md](docs/usage.md)). Versions stay at 0.x until 1.0 is declared on purpose, and one
version covers the whole repository.

## Example

Given this page, [`fixtures/pages/sign-in.html`](fixtures/pages/sign-in.html):

```html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Sign in</title>
</head>
<body>
<h1>Sign in</h1>
<p>
<label for="email">Email</label>
<input id="email" type="email" autocomplete="email">
</p>
<p>
<button id="submit" type="button"><svg width="16" height="16" viewBox="0 0 16 16"><path d="M2 8h12"/></svg></button>
</p>
<p><a id="help" href="help.html">Trouble signing in?</a></p>
</body>
</html>
```

`pnpm -s spokenpath fixtures/pages/sign-in.html`, run from a checkout, prints this transcript:

```text
spokenpath 0.0.0 · snapshot v0.0 · Chromium 159.0.8154.6 · viewport 1280x720
A = what the browser exposes   ~ = illustrative speech, approximate, not any specific screen reader

 1  A RootWebArea (internal role) "Sign in"  focusable, focused
      no source location: the document itself (document)
      name from title (related-element)
      other sources: aria-labelledby (related-element): empty; aria-label (attribute): empty; aria-label (attribute): superseded
    ~ "Sign in"
 2    A heading "Sign in"  level 1
        sign-in.html:8:1  <h1>
        name from contents
        other sources: aria-labelledby (related-element): empty; aria-label (attribute): empty; title (attribute): superseded
      ~ "Sign in, heading, level 1"
 3      A text "Sign in"
        ~ "Sign in"
 4    A paragraph (no accessible name)
        sign-in.html:9:1  <p>
        no accessible name; tried: aria-labelledby (related-element): empty; aria-label (attribute): empty
      ~ "paragraph"
 5      A LabelText (internal role) (no accessible name)
          sign-in.html:10:1  <label for="email">
          no accessible name; tried: aria-labelledby (related-element): empty; aria-label (attribute): empty; title (attribute): empty
        ~ ""
 6        A text "Email"
          ~ "Email"
 7      A textbox "Email"  focusable
          sign-in.html:11:1  <input id="email" type="email" autocomplete="email">
          name from label-for (related-element)
            via <label>  "Email"  sign-in.html:10:1
          other sources: aria-labelledby (related-element): empty; aria-label (attribute): empty; title (attribute): superseded; placeholder (placeholder): superseded; aria-placeholder (placeholder): superseded; title (attribute): superseded
        ~ "Email, textbox"
 8        A generic (no accessible name)
            no source location: its origin could not be established (unknown)
            no accessible name; tried: aria-labelledby (related-element): empty; aria-label (attribute): empty
          ~ ""
 9    A paragraph (no accessible name)
        sign-in.html:13:1  <p>
        no accessible name; tried: aria-labelledby (related-element): empty; aria-label (attribute): empty
      ~ "paragraph"
10      A button (no accessible name)  focusable
          sign-in.html:14:1  <button id="submit" type="button">
          no accessible name; tried: aria-labelledby (related-element): empty; aria-label (attribute): empty; label (related-element): empty; contents: empty; title (attribute): empty
        ~ "button"
11        A image (no accessible name)
            sign-in.html:14:35  <svg width="16" height="16" viewBox="0 0 16 16">
            no accessible name; tried: aria-labelledby (related-element): empty; aria-label (attribute): empty; title (related-element): empty; title (attribute): empty
          ~ "image"
12    A paragraph (no accessible name)
        sign-in.html:16:1  <p>
        no accessible name; tried: aria-labelledby (related-element): empty; aria-label (attribute): empty
      ~ "paragraph"
13      A link "Trouble signing in?"  focusable
          sign-in.html:16:4  <a id="help" href="help.html">
          name from contents
          other sources: aria-labelledby (related-element): empty; aria-label (attribute): empty; title (attribute): superseded
        ~ "Trouble signing in?, link"
14        A text "Trouble signing in?"
          ~ "Trouble signing in?"

Excluded
  <html>  sign-in.html:2:1  <html lang="en">
    uninteresting (engine reason: uninteresting)
  <head>  sign-in.html:3:1  <head>
    not-rendered (engine reason: notRendered)
  <meta>  sign-in.html:4:1  <meta charset="utf-8">
    not-rendered (engine reason: notRendered); hidden by <head>  sign-in.html:3:1 (computed by tool)
  <title>  sign-in.html:5:1  <title>
    not-rendered (engine reason: notRendered); hidden by <head>  sign-in.html:3:1 (computed by tool)
  <body>  sign-in.html:7:1  <body>
    uninteresting (engine reason: uninteresting)
  <path>  sign-in.html:14:83  <path d="M2 8h12"/>
    uninteresting (engine reason: uninteresting)

Headings
  [2] heading "Sign in"  level 1

Landmarks
  (none)

Links
  [13] link "Trouble signing in?"

Form controls
  [7] textbox "Email"
  [10] button (no accessible name)

Tab order
  1  [7] textbox "Email"
  2  [10] button (no accessible name)
  3  [13] link "Trouble signing in?"
  4  left the page
  5  [7] textbox "Email"  repeats step 1
  ending (computed by tool): Focus left the page and Tab returned to step 1, the normal end of a walk.
```

This is the committed end-to-end golden,
[`fixtures/transcripts/sign-in.txt`](fixtures/transcripts/sign-in.txt), with the spokenpath version
in the header normalised to `0.0.0`; the `e2e` check fails when the command's output differs from
it. Lines tagged `A` are what Chromium exposes. Lines tagged `~` are illustrative speech,
approximate. Item 10 is a button with no accessible name: its only content is an SVG image (item 11) that has
no name either, and the transcript lists every name source Chromium tried and found empty.
[docs/usage.md](docs/usage.md) explains every line, the options, the JSON output and the exit
codes.

## What it is

spokenpath is designed to load a page in a real browser, record what the browser exposes, and
print it as a transcript in which every line says where it came from.

- **Layer A, "what the browser exposes".** For each node: its role, name, description and states,
  why a node is excluded from the accessibility tree, and where its name came from (for example a
  `<label>`, `aria-label` or the element's contents). Layer A is labelled with the browser engine
  and version it was captured from. Values the tool computes itself are labelled "computed by
  tool".
- **Layer B, "illustrative speech, approximate".** A neutral phrase per node that suggests how the
  exposed data might be spoken. It is always marked approximate, kept under `approx` in JSON
  output, and removable with `--no-speech`. Phrasing attributed to a real screen reader is planned
  only for patterns backed by captured speech, with the versions stated.
- **Source tags.** Each line is traced to the tag in your HTML that produced it, as
  `file:line:column`.
- **Navigation.** Indexes of headings, landmarks, links and form controls, and the tab order found
  by pressing Tab in the browser.

Layer A, Layer B, source tags and navigation work for Chromium from a checkout, through the
`spokenpath` command ([docs/usage.md](docs/usage.md)). On a Mac, `--cross-check safari` also asks
real Safari for each element's role and name and notes where they differ from Chromium's, without
saying which engine is right. Nothing is published yet, and Safari's own accessibility tree and
VoiceOver phrasing are not built. The [roadmap](docs/roadmap.md) lists the order the rest is being
built in.
The design is recorded in [ADR-0001](docs/decisions/ADR-0001-two-layer-output.md).

## What it is not

spokenpath is not a screen reader, and it is not a substitute for testing with real screen readers
and with the people who use them. It is not a rule checker or a conformance tool, and it makes no
WCAG or other conformance claim. Its output does not reproduce what a screen reader says or what
users experience.

Specifically:

- **Exact only for the named browser build.** Layer A is what one browser version exposes,
  including that browser's bugs. It is not claimed to conform to the specifications.
- **Chromium's tree is Blink's internal tree**, not the platform accessibility API that screen
  readers read (UIA, IA2, the macOS accessibility API, ATK).
- **Safari's tree is the macOS accessibility tree as read by a helper** process, which is close to
  what VoiceOver consumes but is still read from outside VoiceOver.
- **VoiceOver phrasing is shown only for captured patterns** on the stated macOS, Safari and
  VoiceOver versions, and real speech depends on verbosity and other user settings.
- **iOS is not covered.**
- **One moment in time.** A capture is a single snapshot of the page: no live regions and no
  dynamic updates.
- **Script-created and implied elements are unmapped**: they have no source location, and the
  output says so.
- **No iframes.**
- **Captured pages execute their scripts**, because capture loads them in a real browser. Capture
  only pages you trust (see [Security](#security)).
- **Output can change between browser and macOS versions**, which is why every transcript names
  the versions it came from.

## How it works

```mermaid
flowchart TB
  accTitle: The spokenpath pipeline
  accDescr {
    An HTML page is tagged with source locations by instrument-html, captured in a real
    browser, stored as a versioned JSON snapshot, interpreted by the Rust core and rendered
    as a transcript.
  }
  page["HTML page"] --> instrument["@spokenpath/instrument-html"]
  instrument --> capture["Capture in a real browser"]
  capture --> snapshot["Versioned JSON snapshot"]
  snapshot --> core["Rust core"]
  core --> transcript["Transcript, text or JSON"]
```

The same pipeline, step by step:

1. **Instrument.** `@spokenpath/instrument-html` uses a parser to find each start tag and splices a
   source-location attribute into the original text. The page is never re-serialised, and the
   instrumented page's DOM is checked to be equal to the original's apart from that attribute.
2. **Capture.** A real browser loads the instrumented page. Capture records the raw DOM, the raw
   accessibility tree and a real Tab-key focus walk, and interprets nothing.
3. **Snapshot.** Capture writes everything into a versioned JSON snapshot labelled with the engine,
   its version and the viewport.
4. **Rust core.** A per-engine adapter reads the snapshot into an engine-neutral model, and the
   core works out reading order and the navigation indexes.
5. **Transcript.** The core renders the result as text or JSON.

The reasons for this design are in
[ADR-0002](docs/decisions/ADR-0002-browser-ground-truth-and-rust-core.md) and
[ADR-0003](docs/decisions/ADR-0003-source-tagging-by-splicing.md).

## How it compares

Each of these tools does something spokenpath does not, and most of them work today. spokenpath's
side of each comparison describes its design, not a shipped product. The
[market landscape research](docs/research/market-landscape.md) has the full table, with sources and
the date each fact was read.

- **[axe-core](https://github.com/dequelabs/axe-core)** is a rule engine: it finds rule violations
  in a page. spokenpath does not check rules at all. It shows what the browser exposes, and
  leaves rule checking to tools like axe-core.
- **[Guidepup](https://github.com/guidepup/guidepup)** drives real screen readers (VoiceOver on
  macOS, NVDA on Windows, Orca on Linux) and produces their real output. spokenpath runs no screen
  reader; its speech layer is illustrative unless backed by captured speech.
- **[Guidepup's `virtual-screen-reader`](https://github.com/guidepup/virtual-screen-reader)**
  walks the DOM and emits spoken-phrase logs inside Jest and Playwright-style tests. It runs in
  tests today and is widely used. Its output is one generic phrasing, its README says it is not a
  substitute for testing with real screen readers, and it has no source mapping. spokenpath reads
  the accessibility tree from a real browser and traces each line to its source tag.
- **[`@reticular/speakable`](https://github.com/R3TICULAR/AnnounceKit)** predicts what NVDA, JAWS,
  VoiceOver and Narrator would announce from static HTML, through jsdom and per-reader renderers.
  It ships those renderers, a CLI, an MCP server and a Storybook add-on. It describes itself as
  heuristic and publishes no validation against real screen readers. spokenpath uses a real
  browser as its source and shows per-reader phrasing only where captured speech backs it.
- **[Playwright ARIA snapshots](https://playwright.dev/docs/aria-snapshots)** are built into the
  Playwright test runner: a YAML view of the accessibility tree that `toMatchAriaSnapshot` compares
  in tests. They show a tree, not announcements, and cannot map back to source. spokenpath adds
  where each name came from, the source tag, and the browser version.

## Roadmap

Next comes the groundwork (workspaces, probes and the snapshot schema), then source tagging, a
Chromium slice through to a command-line tool, and then Safari and a VoiceOver speech profile on
macOS. The [roadmap](docs/roadmap.md) lists the ordered work and the later items.

## Versioning and releases

- Every commit message and pull request title is a
  [Conventional Commit](https://www.conventionalcommits.org/).
- Pull requests are squash-merged, and the pull request title becomes the commit subject on `main`.
- Every push to `main` runs [semantic-release](https://github.com/semantic-release/semantic-release),
  which tags and publishes a GitHub release when the commits since the last tag warrant one.
- While the version is 0.x, `feat` and breaking changes cut a minor release, `fix`, `perf` and
  GitHub-style reverts cut a patch release, and other types cut none. The
  [contributing guide](CONTRIBUTING.md#commits-and-pull-request-titles) has the full table.
- One version covers the whole repository: every crate and package moves in lockstep.

Releases appear on the [releases page](https://github.com/nickvirden/spokenpath/releases). The
design is recorded in
[ADR-0004](docs/decisions/ADR-0004-conventional-commits-and-automated-releases.md).

## Contributing

Contributions are welcome, especially reports where spokenpath's output disagrees with a real
browser or screen reader. Read the [contributing guide](CONTRIBUTING.md) for setup, checks and
commit rules, and the [code of conduct](CODE_OF_CONDUCT.md) that everyone taking part follows.

## Security

Report vulnerabilities privately as described in the [security policy](SECURITY.md), never in a
public issue. The policy also explains the trust model: capture runs the page's scripts in a real
browser, so capture only pages you trust.

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT licence ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
the work by you, as defined in the Apache-2.0 licence, shall be dual licensed as above, without any
additional terms or conditions.
