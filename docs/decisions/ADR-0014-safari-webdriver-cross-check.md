# ADR-0014: Cross-checking role and name against real Safari over WebDriver

## Status

Accepted

## Date

2026-11-19

## Context

The transcript shows what Chromium exposes. Most desktop VoiceOver users pair it with Safari
([ADR-0006](ADR-0006-engine-scope.md)), and WebKit computes roles and names on its own, so a
reviewer wants each element's Chromium role and name checked against what real Safari reports.
Step 2 of the engine scope is that cross-check.

The probe in [ADR-0009](ADR-0009-safari-capture-route-and-source-locations.md) settles the route:
one `safaridriver` session, then Get Computed Role and Get Computed Label for each tagged element,
each call recorded with its raw error when it fails, and elements compared only where both engines
expose them. Its evidence (`spikes/safari-webdriver/output/webdriver.json`, Safari 27.0.1 on macOS
27.0.1, build 26A434) shows 23 of 24 elements answering both calls in 1.8 to 31.2 ms, an
`optgroup` failing both with `unknown error` and an empty message, a `label` answering an empty
role and label, and roles that differ from Chromium's for common elements: `select` answers
`button`, an `img` with `usemap` an empty role, and a `ul` with `list-style: none` `group`.
ADR-0009 says the cross-check reports these as Safari differences and does not correct them.

The snapshot format ([ADR-0011](ADR-0011-snapshot-format-and-versioning.md)) already has a
`safari-webdriver` payload with no fields. The command-line tool
([ADR-0013](ADR-0013-command-line-tool-and-transcript-output.md)) writes one snapshot to the
binary's stdin and writes no temporary file.

A cross-check must not say which engine is right, and an element Safari could not answer for must
never pass as agreement.

## Decision

1. **Payload fields, in minor version 1.** The `safari-webdriver` payload gains `elements`, one
   entry per element in Safari's DOM whose `data-spokenpath-src` value names a span, in document
   order. Each entry has `span`, exactly one of `role` (a string, possibly empty) and `role_error`,
   and exactly one of `label` and `label_error`; each error is `{ error, message }` as Safari
   returned it, without the stack trace. The payload also has `safari_build` (Safari's
   `CFBundleVersion`) and `macos_version` and `macos_build` (from `sw_vers`). Safari's own version
   is `capture.engine_version`, the session's `browserVersion`. Every new field reads with a
   default, so a minor-0 Safari envelope with an empty payload still reads. The Rust reader rejects
   an entry with both or neither of an answer and its error, and an entry whose span is not in
   `spans`, naming the entry's dotted path.
2. **Each writer writes the lowest minor version that defines every field it writes.** Safari
   capture writes `{ "major": 0, "minor": 1 }`. Chromium capture keeps writing
   `{ "major": 0, "minor": 0 }`, because its payload uses no minor-1 field, so every Chromium golden
   and the sign-in transcript, whose header names the snapshot version, stay unchanged.
   `CURRENT_VERSION`, the newest version the release defines, is `{0, 1}`.
3. **Same page, or no comparison.** The binary compares only when the Safari snapshot's `sources`
   and `spans` equal the Chromium snapshot's, so a span id names the same start tag in both;
   otherwise it exits 2 with one line saying the cross-check snapshot was captured from a
   different page.
4. **What is compared.** Only items that are accessible nodes located by a span. Text runs, the
   document root and implied, script or unknown-origin nodes get no cross-check field, and
   elements Chromium excludes get no note.
5. **How each field compares.** Role and name are compared separately, each with exactly one
   outcome: `same`, `differs` or `not-checked`. Chromium's role token and Safari's computed role,
   and Chromium's name text and Safari's computed label, are compared as exact strings, with no
   synonym mapping, case folding or whitespace normalisation; an absent Chromium role or name
   compares as `""`. A field is `not-checked`, never `same`, when Safari answered that call with
   an error, when Safari's snapshot has no entry for the span (an element inside a shadow root,
   which Find Elements does not search, is one), or when more than one Chromium item or more than
   one Safari entry has the span.
6. **The note wording.** The text transcript gets a second header line,
   `cross-check · Safari <version> on macOS <version> (<build>) over WebDriver · role and name of N
   elements compared: D differ, U not checked`. Each compared item with a field that differs or is
   not checked gets one line per such field at its detail indent, after its description and before
   its `~` line: `differs in Safari: role "group" in Safari 27.0.1, "list" in Chromium
   159.0.8154.6`, or `not checked in Safari 27.0.1: role (Safari answered with error "unknown
   error")`, with `: <message>` appended inside the parentheses when Safari gave one. When both
   fields are not checked for the same reason, one line covers both: `not checked in Safari 27.0.1:
   role and name (no element with this tag was found in Safari)`, or `(more than one element has
   this tag)`. Items whose fields are all the same get no line. The notes print with `--no-speech`
   too. No line says which engine is right. Every Safari-originated string is printed through
   `escape_for_terminal`; the JSON keeps them raw.
7. **The transcript fields.** `Transcript` gains an optional `cross_check` with `engine`,
   `engine_version`, `safari_build`, `macos_version`, `macos_build` and the counts `compared`,
   `differs` and `not_checked`; `Item` gains an optional `cross_check` with `role` and `name`, each
   `{ outcome, safari?, chromium?, error? }`, where `error` is Safari's `{ error, message }` or the
   reason `no-safari-element` or `span-not-unique`. All are absent when no cross-check ran, so
   output without one is byte-identical to what it was before.
8. **A second input route, amending ADR-0013 Decision 2 by addition.** `spokenpath-core transcript`
   takes `--cross-check <snapshot | ->`, read under the same 32 MiB limit, UTF-8 check and version
   handling as the snapshot; at most one of the two may be `-`. `spokenpath <page.html>
   --cross-check safari` captures in Chromium, then in Safari, and runs the binary with the
   Chromium snapshot on stdin and the Safari snapshot on an extra pipe at fd 3, named to the binary
   as `/dev/fd/3`. No temporary file is written, and the binary still opens only the files named
   on its command line.
9. **Loopback serving with a Content-Security-Policy.** WebDriver cannot intercept requests, so
   Safari capture serves the instrumented page, which carries no class token, over HTTP on
   `127.0.0.1` on a port the OS chooses, for the length of the capture. Every response carries a
   Content-Security-Policy that limits loads to the page's own origin while allowing inline and
   `data:`/`blob:` content, so pages behave as they do under Chromium capture, where requests to
   other origins are aborted. A request naming another host is refused. Blocked requests are not
   recorded as diagnostics; the Chromium snapshot of the same run records them.
10. **When Safari capture fails as a whole, there is no transcript.** A refused session, a missing
    `safaridriver`, a timeout or a hidden page (a locked or sleeping Mac) makes the command print
    `spokenpath: <code>: <message>` and exit 1, because a transcript printed without the
    cross-check the user asked for is easy to misread as agreement. A failed call for one element
    still gives `not checked` for that field. SIGINT or SIGTERM during Safari capture deletes the
    session and stops `safaridriver` before the command exits 130 or 143.
11. **No macOS CI job in this change.** CI covers the feature through the committed Safari
    snapshots, which the render goldens read on every platform. A macOS job is a separate change
    that pins its image and re-checks these results on it (ADR-0009 Decision 6); the hosted Safari
    answers differently from the local one (a `ul` with `list-style: none` answers `list` there and
    `group` locally), so it could not compare against goldens captured on the maintainer's Mac.

## Consequences

- `--cross-check safari` runs only on macOS with Safari remote automation enabled, opens a Safari
  automation window, and needs the Mac awake and unlocked: Safari reports a locked Mac's pages as
  hidden, where animation frames never run, so capture fails fast with `page-hidden` instead of
  waiting for its time limit.
- The Safari goldens under `fixtures/snapshots/safari-webdriver/` are captured on the maintainer's
  Mac with `pnpm test:safari`, which is not part of `pnpm check` or CI. A Safari or macOS upgrade
  shows as a reviewed diff of them ([ADR-0007](ADR-0007-golden-test-strategy-and-drift-reports.md)).
- The cross-check text goldens for every Safari golden, the hand-written pair
  `fixtures/snapshots/chromium-basic.json` and `safari-webdriver-basic.json`, and the per-field
  hostile-string test run without Safari on every platform.
- Differences are reported as found and not filtered, so the notes include cases where the two
  engines name the same thing differently, such as Chromium's internal `LabelText` role against
  the empty role Safari answers for a `label`.
- Descriptions, states, values and exclusions are not compared.

## Rejected alternatives

- **Two JSON documents concatenated on stdin.** It needs a framing rule in a reader that has none,
  and a framing error could make the second document look like trailing garbage of the first.
- **A temporary file for the Safari snapshot.** ADR-0013 rejects it for the same reasons: a
  writable location, cleanup on every exit path, and page data left on disk when cleanup fails.
- **Mapping role synonyms** (`img` and `image`, `LabelText` and an empty role). It decides which
  differences matter, which is a judgment about which engine is right, and it hides changes in
  either engine. ADR-0009 says differences are reported, not corrected.
- **Per-element outcomes.** A failed role call would hide a label Safari did answer; per-field
  outcomes are truthful about what Safari answered, at the cost of a second line in the rare mixed
  case.
- **Falling back to a transcript without notes when Safari capture fails.** It is easy to misread
  as a cross-check that found nothing.
- **Recording blocked requests through a CSP `report-uri`.** It adds an endpoint to the server
  for diagnostics the Chromium snapshot of the same run already has.
- **A macOS CI job now.** See Decision 11.
