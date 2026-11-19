# ADR-0015: Capturing Safari's macOS accessibility tree

## Status

Accepted

## Date

2026-11-19

## Context

Step 3 of the engine scope ([ADR-0006](ADR-0006-engine-scope.md)) is Safari's macOS accessibility
tree, the tree macOS assistive technology reads from Safari. Capture records it raw and the Rust
core interprets it later ([ADR-0002](ADR-0002-browser-ground-truth-and-rust-core.md)); this
decision covers capture and the snapshot only.

The probe in [ADR-0009](ADR-0009-safari-capture-route-and-source-locations.md) settles the route
(Decision 2): a native helper reads the tree through Safari's application element, finds the
`AXWebArea` by its `AXURL`, retries briefly while the web area does not exist yet, records
`AXIsProcessTrusted()` and each query's AX error, treats an untrusted process or
`kAXErrorAPIDisabled` (`-25211`) as a missing permission with its own exit status, never infers a
missing permission from an empty count alone, and puts a deadline on every read. Source locations
travel as the class token `sp-src-<line>-<column>`, read back through `AXDOMClassList`
(Decision 3), and `data-spokenpath-src` stays (Decision 4). The probe's evidence
(`spikes/safari-ax-tree/output/`) shows 72 nodes on the first query, 87 distinct attribute names
including display-dependent ones (`AXPosition`, `AXSize`, `AXFrame`, `_AXPrimaryScreenHeight`,
`AXVisibleCharacterRange`, `AXLayoutCount`), opaque values such as `AXStartTextMarker`, element
references, and one trusted run that hung reading the web area until its 60 s deadline.

The Accessibility permission lets a process read any app's interface, so the helper must read
only the automation window capture opened, and a raw tree can hold the page's text and URLs.
Every attribute name the API reports is recorded, so the format must carry any Core Foundation
value without loss. The probe walked every window's interface to find the web area, which this
decision forbids.

Checked on the maintainer's Mac (macOS 27.0.1, 26A434; Safari 27.0.1, 22625.1.29.11.28) before
the window rule was written: Safari's windows report `AXDocument` with no value (`-25212`), while
each window's `AXTitle` equals WebDriver Get Title and its `AXPosition` and `AXSize` equal Get
Window Rect.

## Decision

1. **The payload, in minor version 2.** The `safari-ax` payload gains `nodes`, in pre-order
   through `AXChildren` from the web area (node 0), and `safari_build`, `macos_version` and
   `macos_build` as in the `safari-webdriver` payload. A node has `id` (its index), `parent`
   (absent for node 0), `children`, an optional `span`, and `attributes`: every name
   `AXUIElementCopyAttributeNames` returns, keyed by name, each with its value or the error reading
   it returned. Values are tagged by `type` after their Core Foundation type: `string`, `boolean`,
   `number` (integer or float as stored), `url`, `array`, `element` (`node`, the id of the recorded
   node that is the same element by `CFEqual`, or nothing for an element outside the tree, which
   records no other data), `point`, `size`, `rect`, `range` (exact integers), `unsupported`
   (`cf_type`, the `CFCopyTypeIDDescription`) and `error` (`code`, the `AXError`). An element met
   again in the walk is not walked again and is not listed in `children` a second time. There are
   no fields for name provenance or exclusion reasons, because the platform has none. Every field
   reads with a default. The Rust reader rejects a node whose `id` is not its index and any
   `parent`, `children` entry, element reference or `span` that names nothing, by dotted path.
   `CURRENT_VERSION` is `{0, 2}`; Safari AX capture writes minor 2, while Chromium capture keeps
   writing minor 0 and Safari WebDriver capture minor 1
   ([ADR-0011](ADR-0011-snapshot-format-and-versioning.md), ADR-0014 Decision 2).
2. **The helper.** `spokenpath-ax-macos`, a Rust binary from `crates/spokenpath_ax_macos`, with
   hand-written FFI to `HIServices`, Core Foundation, Core Graphics and the Objective-C runtime in
   one module, the only one in the workspace that allows `unsafe` code, every block with its
   safety invariant, every owned Core Foundation object released by an RAII wrapper. It adds no
   dependency. Its commands each print one JSON document on stdout:
   `read --url <url> --title <title> --window-rect <x>,<y>,<width>,<height> --timeout-ms <n>
   [--canary]`, which prints `{"nodes":[...]}` or with `--canary`
   `{"canary":{"expected":K,"found":K}}`; and `canary-page`, which prints `{"html":"..."}`, the
   single source of the check page. `--version` prints `spokenpath-ax-macos <version>`, as
   `spokenpath-core --version` prints its line. A failure prints
   `{"error":{"code":"<code>","message":"<message>"}}` on stdout and
   `spokenpath-ax-macos: <code>: <message>` on stderr. Exit statuses: 0 on success, 1 for any
   failure but a missing permission, 2 for a usage error, 3 for a missing permission. Off macOS
   the crate is a stub whose `read` fails with `unsupported-platform`.
3. **Read scope and window identification.** The helper reads through the application element of
   the single process with bundle identifier `com.apple.Safari` in the user's session, from
   `NSRunningApplication`, and fails with `safari-not-running` when there is none or more than one.
   It never creates the system-wide element or an element for another process, never sets an
   attribute, and never calls `AXIsProcessTrustedWithOptions`. From the application's `AXWindows`
   it reads only each window's top-level `AXTitle`, compared with the page's title by `CFEqual`
   without being copied out, and for a window with that title its `AXPosition` and `AXSize`,
   compared with the window rect within half a point. Exactly one window must match; only then
   does it descend, through that window's own interface without entering any web area, to the
   web areas, and exactly one must have an `AXURL` equal to `--url` character for character,
   nonce included. Every other case is `safari-window-not-identified`, nothing is recorded, and
   no message quotes anything read from a window. Capture passes the title and rect from WebDriver
   Get Title and Get Window Rect, read just before the helper runs. The web area's `AXURL` is read
   again after the walk; a change is `page-left-origin`. `--url` must be an http URL on
   `127.0.0.1`.
4. **Permission and lock checks, before any tree read.** `AXIsProcessTrusted()` false, or
   `kAXErrorAPIDisabled` from the first application-level read, is
   `accessibility-permission-missing` with exit status 3. The session's `CGSSessionScreenIsLocked`
   is read before and after the read, and a locked screen is `screen-locked`, which capture
   reports as `page-hidden`. The permission message is a reason clause for the signal that fired
   followed by one how-to clause, and claims no cause the helper did not observe.
5. **The check page and its outcomes.** Before the page, capture loads the helper's check page,
   whose 3 elements each carry one token `sp-canary-1` to `sp-canary-3`, in the same session with
   its own nonce, settles it and runs `read --canary`. All 3 tokens in an `AXDOMClassList` is a
   usable read. None, once the process is trusted and the screen unlocked, is
   `accessibility-permission-missing` with its own reason clause, because macOS can return an
   empty tree to a process without the permission. One or two is `ax-canary-incomplete`, which is
   neither a usable read nor a missing permission.
6. **Caps and deadlines.** The walk records at most 50 000 nodes (`ax-tree-too-large` beyond).
   Identification repeats every 250 ms for up to 5 s while no matching window or web area exists
   yet. `AXUIElementSetMessagingTimeout` bounds each AX message to 5 s, set on the application
   element and on each element before it is read. The overall `--timeout-ms` is checked between
   reads (`timeout`). Capture reads at most 32 MiB of the helper's stdout, the input limit of
   `spokenpath-core`, killing it and failing with `ax-tree-too-large` beyond; kills it at
   `timeoutMs` plus 5 s with `timeout`; and on SIGINT or SIGTERM kills it before deleting the
   session. Any output that is not one of the documented shapes, or whose exit status disagrees
   with its code, is `ax-helper-failed`, quoting the first 4 KiB of stderr escaped.
7. **The nonce.** Capture serves the page at `<server url>?spokenpath-nonce=<32 hex>`, from 16
   random bytes per capture, and that exact URL is `capture.url` and the helper's `--url`, so an
   automation window left from an earlier run on a reused port cannot be the sole match.
8. **Source tokens.** `@spokenpath/instrument-html` gains `classToken: true`, which adds
   `sp-src-<line>-<column>` to each tagged element's first `class`, or in a new `class`, and whose
   equality proof ignores exactly that one change per tagged element. A page whose class already
   holds a `sp-src-` token is refused in that mode with `class-token-present`. Without the option
   the output is byte-identical. Capture gives each node the span its token names (source 0) and
   records `unknown-source-token` for a token that names none; the recorded values keep the token.
9. **Normalisation.** `normaliseSafariAxForGolden` applies `normaliseSafariForGolden`, sets the
   nonce in `capture.url` to `0`, sets the served port and the nonce to `0` in every `string` and
   `url` value, and zeroes `AXPosition`, `AXFrame`, `AXSize`, `_AXPrimaryScreenHeight`,
   `AXVisibleCharacterRange` and `AXLayoutCount`, keeping their types. Two consecutive live
   captures of every page agree on every other attribute.
10. **The source-mapping figure.** The figure is `mapped / (mapped + unmapped_in_tree)`, where
    `mapped` counts the spans at least one Safari node carries and `unmapped_in_tree` the spans no
    Safari node carries that Chromium's golden of the same page exposes as a non-ignored node.
    Spans absent from both are listed with their reason and left out. Over the 46 committed pages
    it is **425 / (425 + 18) = 95.9%**: 711 spans, 425 mapped, none carried by more than one node,
    268 not in either tree and 18 exposed in Chromium only, all listed in
    `fixtures/snapshots/safari-ax/source-mapping.json`, which a `pnpm test:node` test recomputes
    and holds at 95% or more.

## Consequences

- Capturing the tree needs macOS, Safari remote automation, the Accessibility permission for the
  app that runs the capture, and an awake, unlocked Mac. The live goldens under
  `fixtures/snapshots/safari-ax/` are captured on the maintainer's Mac with `pnpm test:safari`,
  which builds the helper first and stays outside `pnpm check` and CI; a Safari or macOS upgrade
  shows as a reviewed diff of them.
- CI builds, lints and tests the helper on Linux and Windows as the stub, and on macOS lints the
  real FFI path with clippy; no CI job drives Safari.
- The window rule depends on WebDriver and the accessibility API agreeing on the window's title
  and rect. Two windows with the same title and rect fail closed rather than guessing. A Safari
  that begins to report `AXDocument` changes nothing here.
- The goldens are large, because every attribute of every node is kept. They compare every
  attribute but the display-dependent ones, so they also catch changes in what Safari reports.
  The Mac's appearance was not varied while the normalisation list was drawn up, because that is
  a setting of the maintainer's Mac.
- Range integers beyond 2^53, such as `NSNotFound`, are exact in Rust and approximate in a
  JavaScript reader.
- The `safari-ax` adapter, role mapping and per-engine transcripts are the next step.
  `spokenpath-core` keeps rejecting a `safari-ax` snapshot until then.
- The 18 spans no Safari node carries although Chromium exposes them are listed with the figure,
  by page: the `optgroup` and two `option`s of a `select` and its `body`
  (`cross-check/select-optgroup`), two more `option`s (`navigation/landmarks`), a `table` and a
  `tr` on each of `cross-check/tables` and `origins`, a `span` on each of
  `cross-check/owns-in-name`, `names/description-describedby` and `names/labelledby-over-label`, a
  `div` on each of `focus/closed-shadow` and `reading-order/aria-owns`, the `iframe` of
  `focus/iframe`, the `legend` of `set-position/radio-group` and the inline `svg` of `sign-in`.

## Rejected alternatives

- **The window's `AXDocument`.** Safari reports it with no value, so it cannot pick the window.
- **Walking every window to find the web area, as the probe did.** It reads other windows'
  interfaces, which the read scope forbids.
- **Safari's attribute names kept in their order beside the values.** The order carries no meaning
  in the API, and a keyed object is stable for goldens and lossless for every value.
- **`objc2` crates for the Objective-C and Core Foundation calls.** They add pinned dependencies to
  reach one class method and one property; the hand-written calls are a few lines in the same
  audited module.
- **Relying on the OS-chosen port alone to make the URL unique.** A stale automation window on a
  reused port could then be the sole match.
- **Treating a partial check page as a missing permission.** The message would claim a cause the
  helper did not observe.
- **Making the caps options.** No request asks for them, and the fixed values keep every snapshot
  readable by the core.
- **Counting only spans carried by exactly one node, as the probe did.** Each node that carries a
  token does carry the element's location; spans carried by several nodes are listed instead.
- **Mapped over all spans, or a hand-reviewed list of spans declared absent from Safari.** The
  first counts elements no engine exposes against Safari; the second can be tuned by hand. The
  Chromium-exposed proxy is mechanical and errs against the bar.
- **A `spokenpath` option for this capture now.** The adapter and the command surface come
  together in the next step.
