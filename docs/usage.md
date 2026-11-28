# Using spokenpath

spokenpath prints what the browser exposes for an HTML page, item by item, each item traced to the
tag in the file that produced it. It runs from a checkout of this repository; nothing is published
or installable yet. Set up the checkout as [AGENTS.md](../AGENTS.md#setup) describes, including the
pinned Chromium (Setup step 5) and the Rust toolchain.

spokenpath shows what one browser build exposes. It is not a screen reader, does not reproduce what
a screen reader says or what users experience, and makes no WCAG or other conformance claim. The
`~` lines are illustrative speech, approximate, not any specific screen reader.

## Commands

### `spokenpath`

```sh
pnpm -s spokenpath <page.html> [--engine chromium|safari|both] [--cross-check safari] [--format text|json] [--no-speech] [--wait-for <selector>] [--verify-instrumentation]
pnpm -s spokenpath -h|--help
pnpm -s spokenpath --version
```

`pnpm spokenpath` builds the `spokenpath-core` binary and the `spokenpath-ax-macos` helper when
they are out of date (`cargo build --quiet --locked -p spokenpath_cli -p spokenpath_ax_macos`; off
macOS the helper is a small stub) and then runs `node packages/cli/src/cli.ts` with the arguments that
follow. `-s` (`--silent`) keeps pnpm's own `> spokenpath-workspace@… spokenpath` banner off stdout,
so stdout holds the transcript alone; without it, pnpm prints the banner before the transcript.

The command instruments the page with its source locations, captures it in the pinned Chromium,
and pipes the snapshot to `spokenpath-core transcript -` on stdin, with the page as `--source`. The
binary's stdout and stderr are the command's, unchanged. No temporary file is written.

With `--engine safari` it captures the page in the Mac's real Safari instead, reading Safari's
macOS accessibility tree through the `spokenpath-ax-macos` helper, and pipes that snapshot to the
binary on stdin ([Safari transcripts](#safari-transcripts)). With `--engine both` it captures in
Chromium, then in Safari, and pipes the Chromium snapshot on stdin and the Safari snapshot on an
extra pipe, fd 3, named to the binary as `--align-with /dev/fd/3`
([Both engines side by side](#both-engines-side-by-side)). With `--cross-check safari` it captures
in Chromium and then asks Safari over WebDriver, and pipes that snapshot on fd 3 as
`--cross-check /dev/fd/3` ([Cross-checking against Safari](#cross-checking-against-safari)).

The extra pipe is a macOS route, where Safari runs: macOS reopens it through `/dev/fd/3`, and Linux
does not. On other systems, give the binary the second snapshot as a file path instead. Off macOS,
`--engine safari`, `--engine both` and `--cross-check safari` exit 2 with `unsupported-platform`
before anything is captured.

| Option | Meaning |
|---|---|
| `<page.html>` | The page to capture. Exactly one is required. |
| `--engine chromium` | The transcript of what Chromium exposes. The default. |
| `--engine safari` | The transcript of Safari's macOS accessibility tree. macOS only, with Safari remote automation on and the Accessibility permission granted. It cannot be combined with `--cross-check` or `--verify-instrumentation`. |
| `--engine both` | Chromium's and Safari's items side by side, aligned by source location. macOS only, as for `safari`, and with the same exclusions. |
| `--cross-check safari` | Also ask real Safari for each element's role and name, and note where they differ from Chromium's. macOS only; `safari` is the only value. Only with `--engine chromium`, and not with `--verify-instrumentation`. |
| `--format text` | The text transcript (the default). |
| `--format json` | The transcript as JSON, for tools. |
| `--no-speech` | Leave out Layer B: no `~` lines and no `approx` in JSON. |
| `--wait-for <selector>` | After the `load` event, wait until a CSS selector matches before capturing. |
| `--verify-instrumentation` | Check that the instrumented page builds the same DOM as the original, print one line, and capture nothing. It cannot be combined with `--format` or `--no-speech`. |
| `-h`, `--help` | Print the usage line and a summary of the options to stdout, and exit 0. |
| `--version` | Print `spokenpath <version>`, the workspace version, and exit 0. |

Each option may be given once; a repeated option is a usage error. `--help`, `-h` and `--version`
take no other arguments.

**Paths.** The page path resolves against the process's working directory. Under `pnpm spokenpath`
that is the repository root, wherever the command is typed, so name pages relative to the root
(`pnpm -s spokenpath fixtures/pages/sign-in.html`) or give an absolute path.

**The binary.** The command runs `target/debug/spokenpath-core` (`spokenpath-core.exe` on Windows)
under the repository root, or `$CARGO_TARGET_DIR/debug/` when `CARGO_TARGET_DIR` is set, resolved
against the repository root when it is relative. When the binary is missing, the command exits 1
and names `cargo build -p spokenpath_cli`.

**Trust.** Capture loads the page in a real browser, which runs the page's scripts. Capture only
pages you trust. Capture serves the page locally and blocks external network requests
([SECURITY.md](../SECURITY.md#trust-model)). Safari capture serves it on a loopback port for the
length of the capture.

### `spokenpath-core`

```sh
target/debug/spokenpath-core transcript <snapshot | -> [--source <file>] [--cross-check <snapshot | ->] [--align-with <snapshot | ->] [--format text|json] [--no-speech]
target/debug/spokenpath-core --version
```

`transcript` reads a snapshot (see [snapshot-format.md](snapshot-format.md)) from a file, or from
stdin when the snapshot argument is `-`, and writes its transcript to stdout. The adapter is
chosen by the snapshot's payload: `chromium` or `safari-ax`. A `safari-webdriver` snapshot holds
answers for a cross-check, not a tree, and exits 2. `--format` and
`--no-speech` mean what they mean for `spokenpath`; each option may be given once. Options are
separate arguments (`--format json`, not `--format=json`). `--version` prints
`spokenpath-core <version>`.

`--source <file>` names the page file the snapshot was captured from. The binary hashes it with
SHA-256 and uses its bytes for the snapshot source with the same `sha256`; a file that matches no
source is an error. The source bytes give the text output its excerpts and let the adapter check
every span against the file's length. Without `--source`, the text output shows locations and no
excerpts. The binary never opens a path written inside a snapshot (`sources[].path` or a span's
path): the only files it opens are the snapshot argument, `--source`, `--cross-check` and
`--align-with`.

`--cross-check <snapshot | ->` names a Safari WebDriver snapshot of the same page (its payload is
`safari-webdriver`), read from a file or from stdin, to compare roles and names with
([Cross-checking against Safari](#cross-checking-against-safari)). At most one of the snapshot
argument and `--cross-check` may be `-`, since stdin holds one snapshot. The cross-check snapshot
must have the same `sources` and `spans` as the snapshot, which it does when both were captured
from the same file. The snapshot must be a `chromium` snapshot.

`--align-with <snapshot | ->` names a `safari-ax` snapshot of the same page, read the same way, and
prints the two transcripts side by side ([Both engines side by side](#both-engines-side-by-side)).
The snapshot must be `chromium`, the aligned one `safari-ax`, and both must have the same `sources`
and `spans`. It cannot be combined with `--cross-check`, and at most one input may be `-`.

**Size limit.** The snapshot, the `--source` file and the cross-check or aligned snapshot may each
be at most
32 MiB. A file's size is checked before it is read and stdin is read no further than the limit, so
a larger input exits 2 without being parsed.

## Exit codes

| Code | `spokenpath` | `spokenpath-core` |
|---|---|---|
| 0 | The transcript was printed, or `--verify-instrumentation` found the same DOM. | The transcript was written. |
| 1 | Capture failed (a timeout, the browser, an invalid snapshot), the binary is missing, `--verify-instrumentation` found a different DOM, or the binary was ended by a signal. With `--engine safari`, `--engine both` or `--cross-check safari`: no `safaridriver` (`safaridriver-not-found`), a refused session (`safari-session-refused`), a hidden page on a locked or sleeping Mac (`page-hidden`), a missing Accessibility permission (`accessibility-permission-missing`), a missing helper (`ax-helper-not-found`), or any other Safari capture failure; no transcript is printed. | Writing stdout failed, for example to a closed pipe. |
| 2 | A usage error, a page that cannot be read (`page-unreadable`), a page the instrumenter refuses (its code, such as `attribute-present`), or `--engine safari`, `--engine both` or `--cross-check safari` off macOS (`unsupported-platform`). Also the binary's 2. | A usage error, or bad input: an unreadable file, an input over 32 MiB, text that is not JSON or not a snapshot, an engine mismatch, a field or reference error (naming its dotted path, such as `` `spans[2].source` ``), a `--source` that matches no source, or a `safari-webdriver` snapshot to print. For `--cross-check` and `--align-with`, a snapshot that is not `chromium`, and the same errors for the second snapshot, named as such, with one whose payload is not `safari-webdriver` (for `--cross-check`) or `safari-ax` (for `--align-with`), naming its engine, or that was captured from a different page. |
| 3 | The binary's 3. | The major version of the snapshot or the second snapshot is not one the reader reads; the message names both majors. |
| 130, 143 | SIGINT (Ctrl-C) or SIGTERM stopped Safari capture; the Safari session was deleted and `safaridriver` stopped first. | |

Every failure writes a message to stderr: `spokenpath: <code>: <message>` from the command,
`spokenpath-core: <message>` from the binary. A usage error is followed by a `usage:` line. A
multi-line message, such as a browser error with its call log, keeps its line breaks, and each line
is escaped as [Terminal escaping](#terminal-escaping) describes.

**Diagnostics.** Problems the capture recorded rather than failed on, such as a blocked external
request, are written to stderr by the binary, one per line, as
`spokenpath-core: diagnostic <code>: <message>`, in both formats, and those of the cross-check or
aligned snapshot with its engine, as `spokenpath-core: diagnostic safari-webdriver <code>: <message>`
or `spokenpath-core: diagnostic safari-ax <code>: <message>`. They change neither
the exit code nor stdout.

## The text transcript

The transcript of `fixtures/pages/sign-in.html` is committed as
[`fixtures/transcripts/sign-in.txt`](../fixtures/transcripts/sign-in.txt). Its parts, line by line:

```text
spokenpath 0.0.0 · snapshot v0.0 · Chromium 159.0.8154.6 · viewport 1280x720
A = what the browser exposes   ~ = illustrative speech, approximate, not any specific screen reader
```

- **Header.** The spokenpath version, the snapshot's version (`v<major>.<minor>`), the engine and
  the browser version the capture came from, and the capture's viewport. With a cross-check, a
  second line follows ([Cross-checking against Safari](#cross-checking-against-safari)).
- **Legend.** `A` lines are what the browser exposes; `~` lines are illustrative speech,
  approximate. With `--no-speech` the legend has the `A` part only.

```text
 7      A textbox "Email"  focusable
          sign-in.html:11:1  <input id="email" type="email" autocomplete="email">
          name from label-for (related-element)
            via <label>  "Email"  sign-in.html:10:1
          other sources: aria-labelledby (related-element): empty; aria-label (attribute): empty; ...
        ~ "Email, textbox"
```

- **Items.** Every exposed node is one item, printed in reading order under its number, which is
  the `number` in the JSON output, and indented two spaces per level of depth. Items deeper than
  32 levels are indented no further and start with `[depth N]`.
- **`A` line.** The role token, marked `(internal role)` when it is the engine's own rather than an
  ARIA role; then the name in double quotes, or `(no accessible name)`; then `value "…"`; then the
  exposed states as words (`checked`, `not checked`, `checked mixed`, `pressed`, `not pressed`,
  `pressed mixed`, `expanded`, `collapsed`, `selected`, `disabled`, `required`, `invalid`,
  `read-only`, `focusable`, `focused`, `level N`); then the set position, such as
  `2 of 3 (computed by tool)`. `(computed by tool)` marks a value the tool derived itself from what
  the browser exposes rather than read from the browser, as
  [ADR-0001](decisions/ADR-0001-two-layer-output.md) defines it. A boolean state that is false is
  left out of the text, except `expanded`, which shows as `collapsed`; the JSON keeps every state.
  A text run's `A` line is `text` and its text in quotes.
- **Location.** For an accessible item, `path:line:column` of its start tag (1-based, the column
  in Unicode scalar values) and the excerpt: the start tag's bytes from the `--source` file,
  decoded as UTF-8 (invalid bytes become U+FFFD), with each run of whitespace collapsed to one
  space and cut to 80 characters with `…`. The excerpt never comes from the browser's DOM. An item
  without a source location says so and why: `implied` (the HTML parser inserted it), `script` (a
  page script created it), `library`, `unknown` (its origin could not be established, such as
  user-agent shadow content), `document` (the document itself) or `not-in-dom` (the snapshot's DOM
  does not hold it, such as a list marker). A text run has no location line.
- **Name provenance.** For a named item, `name from` the source that produced the name, written as
  the attribute or the native mechanism with the source's kind in parentheses (`contents` alone
  when it is the element's contents), and `from hidden content (computed by tool)` when the name
  draws on content the engine hides. Each node the source drew on follows on a `via` line with its
  text and location. `other sources:` lists every other source the engine tried, each with its
  outcome (`empty`, `superseded` or `invalid`). The same source can appear twice, because the
  engine reports it once for each mechanism it tries. For an unnamed item, `no accessible name; tried:`
  lists every source with its outcome, or the line says the engine reported none.
- **Description.** `description "…"` with the attribute it is attributed to and its marking, such as
  `from aria-describedby: <span> #hint  page.html:10:1 (computed by tool)`, or a statement that its
  source is not attributed.
- **Cross-check notes.** With a cross-check, a compared item whose role or name differs in Safari,
  or could not be checked, has one note line per such field after its description
  ([Cross-checking against Safari](#cross-checking-against-safari)).
- **`~` line.** The illustrative phrase in double quotes, exactly as the core rendered it. An item
  whose phrase is empty, such as an unnamed generic container, prints `~ ""`, so the item still
  shows that Layer B said nothing.

**Display decisions.** Nothing is filtered: the root, unnamed generic containers, text runs that
repeat their parent's name (a heading's text) and list markers each print as their own item,
because the indexes and the tab order cite item numbers and every number they cite must resolve.
A list marker prints its name and phrase in quotes as rendered (`"• "`), so a trailing space is
visible. A line break in a text run prints as `\u{a}`. Every unnamed item's `A` line says
`(no accessible name)` beside its role. The text leaves out the role origin and the engine's other
properties; the JSON keeps both.

After the items:

- **Excluded.** Every DOM element the engine leaves out of the exposed tree, in DOM order: its tag,
  its location and excerpt, and each reason, with the engine's own name for it and, when one is
  known, the ancestor that hides it, with that ancestor's location and marking.
- **Headings, Landmarks, Links, Form controls.** Each index entry cites `[number]`, the role and the
  name or `(no accessible name)`; a heading also shows its level. An empty index prints `(none)`.
- **Tab order.** One line per Tab press: `[number]` with the role and name for an exposed element,
  `left the page`, an element that is not an item with its location, or an element the snapshot's
  DOM does not hold, by the engine's node id. The step a repeat returns to is shown as
  `repeats step N`. The last line is the ending, computed by tool, worded as the core renders it.

## The JSON transcript

`--format json` writes one JSON document: the core's `Transcript` serialised as it is, with
`header`, `nodes` (each with `number`, `depth`, `parent`, `layer_a` and, unless `--no-speech`,
`approx`), `indexes`, `excluded` and `tab_order`, and, with a cross-check only, the top-level
`cross_check` and each compared node's `cross_check`. A Safari transcript adds `not_available`,
the list of parts Safari's macOS accessibility tree does not supply (`name-sources`, `excluded`,
`tab-order`), and leaves out each listed part's field: `excluded`, `tab_order`, and `sources` on
every name. Its header adds `safari_build`, `macos_version` and `macos_build`, and a platform role
is `{ "token": "AXGroup", "kind": "platform", "subrole": "AXFieldset" }`, `subrole` present only
when Safari gives one. With `--align-with`, the Chromium transcript gains `aligned`: `safari`,
Safari's whole transcript, and `rows`, each `{ "chromium": [numbers], "safari": [numbers] }`, in
row order. A Chromium transcript has none of these fields. Nothing is added, renamed, removed or
filtered, and strings are the page's raw strings with JSON escaping only. Its JSON Schema (draft
2020-12) is [`schema/transcript.v0.json`](../schema/transcript.v0.json), generated from the Rust
types by `cargo xtask schema` and checked in CI with `cargo xtask schema --check`. The header's
`tool_version` is the version of the tool that drove the browser (Playwright), while the text
header names spokenpath's own version.

Because the JSON keeps the page's raw strings, JSON from a page you do not trust can carry control
characters and escape sequences: pipe it to a tool, and do not print it to a terminal unfiltered.

## Safari transcripts

```sh
pnpm -s spokenpath fixtures/pages/sign-in.html --engine safari
```

`--engine safari` prints Safari's macOS accessibility tree for the page, in the layout of the
Chromium transcript, read from a `safari-ax` snapshot
([ADR-0016](decisions/ADR-0016-safari-adapter-and-per-engine-transcripts.md)). It shows what Safari
exposes to the platform's accessibility API; it is not what a screen reader says or what users
experience. It runs Safari as the cross-check does ([How it runs](#cross-checking-against-safari)),
with the [Accessibility permission](#granting-the-accessibility-permission). Its parts differ from
the Chromium transcript in these lines and no others:

```text
spokenpath 0.0.0 · snapshot v0.2 · Safari 27.0.1 on macOS 27.0.1 (26A434) · viewport 1280x720
 4    A AXGroup (platform role) (no accessible name)
        sign-in.html:9:1  <p>
        no accessible name; name sources not available from this engine
      ~ ""
 5      A AXStaticText (platform role) (no accessible name)  value "Email"
```

- **Header.** The Safari version, then the macOS version and build when the capture recorded them.
- **Items.** The tree is Safari's, in its `AXChildren` order. An `AXStaticText` with no children and
  no source tag, whose `AXValue` is text, is a text run.
- **Role.** `AXRole` with `AXSubrole`. These pairs print as ARIA roles: `AXHeading` `heading`,
  `AXLink` `link`, `AXButton` `button`, `AXCheckBox` `checkbox`, `AXRadioButton` `radio`,
  `AXRadioGroup` `radiogroup`, `AXTextField` and `AXTextArea` `textbox`, `AXImage` `image`,
  `AXList`/`AXContentList` `list`, and `AXGroup` with an `AXLandmark…` subrole as `banner`,
  `complementary`, `contentinfo`, `form`, `main`, `navigation`, `region` or `search`. Every other
  role prints as Safari's text, marked `(platform role)`, as `AXGroup (platform role)` or
  `AXGroup/AXApplicationGroup (platform role)`, because each stands for more than one ARIA role in
  WebKit. A platform role is not spoken in the `~` line and lists an item in no index.
- **Name.** `AXTitle`, or `AXDescription` when the title is empty. Safari does not say where a name
  came from, so the provenance line is `name sources not available from this engine`, or
  `no accessible name; name sources not available from this engine`.
- **Description and value.** `AXHelp` is the description, printed with `(its source is not
  attributed)`; a non-empty `AXValue` is the value, except where it is a heading's level or a
  checkbox's or radio button's checked state. An empty attribute is no value.
- **States.** `disabled` from `AXEnabled`, `focused`, `selected`, `expanded` (`collapsed`),
  `required` and `invalid` from their attributes, `level N` from a heading's `AXValue`, and
  `checked`, `not checked` or `checked mixed` from a checkbox's or radio button's.
- **Location.** As for Chromium, from the element's source tag. A node without one says
  `the document itself (document)` for the web area, `a text node (text)` for static text, and
  `its origin could not be established (unknown)` for anything else, such as table columns or a
  date input's steppers.
- **Excluded** and **Tab order** each read `not available from this engine`: Safari's tree gives
  no exclusion reasons and no focus walk, and none is computed or borrowed from Chromium.

## Both engines side by side

```sh
pnpm -s spokenpath fixtures/pages/sign-in.html --engine both
```

`--engine both` prints Chromium's items and Safari's together, aligned by source location, so
each tag's two `A` lines sit one above the other:

```text
spokenpath 0.0.0 · snapshot v0.0 · Chromium 159.0.8154.6 · viewport 1280x720
aligned with · snapshot v0.2 · Safari 27.0.1 on macOS 27.0.1 (26A434) · viewport 1280x720
A = what the browser exposes   C<n> = Chromium item n, S<n> = Safari item n, aligned by source location

C2   S2   sign-in.html:8:1  <h1>
          A Chromium  heading "Sign in"  level 1
          A Safari    heading "Sign in"  level 1
C11       sign-in.html:14:35  <svg width="16" height="16" viewBox="0 0 16 16">
          A Chromium  image (no accessible name)
            only in Chromium: no Safari item has this source location
```

- **Headers.** The Chromium header, then `aligned with` and the Safari snapshot's version, the
  Safari and macOS versions and its viewport.
- **Rows.** Each block starts with its item numbers, `C<n>` for Chromium's and `S<n>` for
  Safari's, which are the numbers of each engine's own transcript, and the row's location. Then
  one `A Chromium` or `A Safari` line per item, exactly as each engine's transcript prints it. A
  row with a source tag one engine alone has ends with `only in Chromium` or `only in Safari`.
- **What aligns.** Two items share a row when both are located by the same source tag. Text runs
  and items without a tag never align, because nothing says which node of the other engine they
  stand for; each is a row of its own.
- **Row order.** Chromium's reading order. A Safari item in no shared row comes right after the
  rows of the nearest earlier shared Safari item and the Chromium-only rows that follow them, in
  Safari's order; Safari items before the first shared one come right before the first shared row.
- **Nothing else.** No provenance, description, `~`, Excluded, index or tab-order lines; each
  engine's own transcript has them. `--no-speech` leaves the text unchanged.

## Cross-checking against Safari

```sh
pnpm -s spokenpath fixtures/pages/cross-check/list-style-none.html --engine chromium --cross-check safari
```

`--cross-check safari` asks the Mac's real Safari, over WebDriver, for the computed role and the
computed label (the accessible name) of every tagged element, and compares each with what
Chromium exposes for the same element. It shows what each browser exposes and where the two
differ; it does not say which engine is right, and the notes are not a judgment of the page.

**What is compared.** For every item that is an element located by its tag in the file, its role
and its name, separately, each as an exact string: no synonym mapping, case folding or whitespace
changes. An absent Chromium role or name compares as an empty string. Not compared: descriptions,
states and values; the elements Chromium excludes; text runs, the document root, and elements the
parser, a script or the browser created. Elements inside shadow roots show as not checked, because
WebDriver does not search shadow roots.

**The header line.** A second header line names the versions and counts:

```text
cross-check · Safari 27.0.1 on macOS 27.0.1 (26A434) over WebDriver · role and name of 18 elements compared: 2 differ, 1 not checked
```

`differ` counts the items with at least one field that differs, and `not checked` those with at
least one field that could not be checked.

**The notes.** Each compared item with a field that differs or is not checked gets one note line
per such field, at its detail indent, after its description and before its `~` line. They print
with `--no-speech` too. An item whose role and name are the same in both engines gets no line.

```text
differs in Safari: role "group" in Safari 27.0.1, "list" in Chromium 159.0.8154.6
differs in Safari: name "" in Safari 27.0.1, "Prices" in Chromium 159.0.8154.6
not checked in Safari 27.0.1: role (Safari answered with error "unknown error")
not checked in Safari 27.0.1: role and name (no element with this tag was found in Safari)
not checked in Safari 27.0.1: role and name (more than one element has this tag)
```

- **`differs in Safari`.** The field, Safari's value and Chromium's, each with its browser version.
- **`not checked`.** Safari answered the call with an error (its WebDriver error code, and its
  message after a colon when it gave one), Safari's DOM holds no element with the tag, or more than
  one element or item has the tag. A field that was not checked is never counted as the same. When
  both fields are not checked for the same reason, one line names both.

A quoted `""` is the engine's own answer: an empty role or an empty name. A field Safari did not
answer shows as `not checked`, never as `""`. The two engines expose some elements differently, so
some notes appear on most pages: Chromium's internal roles (marked `(internal role)` on the `A`
line, such as `LabelText` and `LayoutTable`) and `generic` show as differences against the empty
role Safari answers, and the names of `option` and `th` elements against the empty name Safari
answers. The counts say how many items differ, not how well the page does.

In JSON, the top-level `cross_check` holds `engine` (`safari-webdriver`), `engine_version`,
`safari_build`, `macos_version`, `macos_build`, `compared`, `differs` and `not_checked`. Each
compared node's `cross_check` holds `role` and `name`, each `{ outcome, safari, chromium }` with
`outcome` `same` or `differs`, or `{ outcome: "not-checked", error }` with Safari's
`{ error, message }` or the reason `no-safari-element` or `span-not-unique`.

**How it runs.** The page is instrumented exactly as for Chromium and served over HTTP on
`127.0.0.1`, on a port the OS chooses, for the length of the capture. A request naming another
host is refused, and every response carries `Cross-Origin-Resource-Policy: same-origin` and a
Content-Security-Policy that limits the page's sub-resource loads and form submissions to that
origin. The policy does not stop top-level navigation (a script setting `location`, a meta
refresh), resource hints or WebRTC.

The command starts `/usr/bin/safaridriver` on a free loopback port, opens one session, loads the
page, waits for it to settle as Chromium capture does, finds the page's tagged elements (WebDriver
Find Elements), reads every element's role and label one after another, then deletes the session
and stops `safaridriver`, on every exit path. Ctrl-C (SIGINT) or SIGTERM during Safari capture does
the same before the command exits 130 or 143.

**Navigation.** A page that navigates away before Find Elements, including one whose own script
navigates as it loads, fails the capture with `page-left-origin`. Safari then reports either the
window's new origin or, for a navigation still under way, `no such window` for the page's script.
A navigation after Find Elements, while the elements are read, is not detected: Safari answers
those reads with `stale element reference`, each such element shows as `not checked` (no element
with this tag was found in Safari), and the command exits 0. None of this prevents the navigation,
so the page may already have made requests to other origins.

**Failures.** A call that capture cannot do without, such as loading the page, that Safari answers
with an error, or a lost session (`invalid session id`, or `no such window` while the elements are
read), fails it with `safari-call-failed`, naming the step. When Safari capture fails as a whole,
the command prints `spokenpath: <code>: <message>`, exits 1 and prints no transcript, so a missing
cross-check is never mistaken for agreement.

### Enabling Safari automation

Safari accepts WebDriver sessions only once remote automation is allowed, once per Mac. Either run

```sh
safaridriver --enable
```

which asks for an administrator password, or, in Safari, turn on Settings > Advanced > "Show
features for web developers", then choose Develop > "Allow Remote Automation". spokenpath never
runs `safaridriver --enable` or changes a setting itself; a refused session fails with
`safari-session-refused` and quotes what `safaridriver` said.

Each run opens a Safari automation window, which closes when the capture ends. Keep the Mac awake
and unlocked while it runs: Safari reports the page as hidden on a locked or sleeping Mac, and a
hidden page does not settle, so capture fails with `page-hidden`.

### Granting the Accessibility permission

Capturing Safari's macOS accessibility tree (`--engine safari` and `--engine both`, and
`pnpm test:safari`) reads it through the `spokenpath-ax-macos` helper, which needs the macOS Accessibility permission.
macOS grants it to the app the helper runs under: the terminal app (such as Terminal or iTerm2),
or the editor or other app that started the command. Grant it once in System Settings > Privacy &
Security > Accessibility by turning that app on, then quit and reopen the app.

spokenpath never prompts for, grants or changes the permission or any other setting, and the
helper reads the tree of the automation window only, never another window's contents or another
app ([SECURITY.md](../SECURITY.md#trust-model) states its read scope). When the
capture cannot read the tree it fails with one of these codes:

- `accessibility-permission-missing`: the app lacks the permission. The message says which sign
  macOS gave: the process is not trusted for Accessibility, macOS refused a request with
  `kAXErrorAPIDisabled` (-25211), or the check page capture loads first came back with none of its
  expected nodes.
- `page-hidden`: the Mac's screen is locked, or Safari reports the page as hidden; the message says
  which.
- `ax-canary-incomplete`: the check page came back with some but not all of its expected nodes, so
  the tree cannot be relied on. Run the capture again, and if it repeats, report it with the
  Safari and macOS versions.

The helper identifies Safari's automation window by the page's title, so a page needs a non-empty
`<title>`. A page without one fails with `safari-window-not-identified` before the helper reads
anything.

Keep the Mac awake and unlocked for the whole run, as for every Safari capture.

## Terminal escaping

Page text reaches the terminal, so every page-controlled string the text output and the messages
on stderr print (names, text, values, attribute values, role tokens, descriptions, paths,
excerpts, Safari's roles, subroles, labels, errors and versions, the macOS version and build,
diagnostics and error messages that echo input) has these characters written as `\u{..}`, the code point in lowercase hexadecimal:

- control characters: C0, DEL and C1, including ESC (`\u{1b}`) and the CSI introducer U+009B, so
  ANSI escape sequences cannot reach the terminal;
- bidirectional controls and marks: U+202A to U+202E, U+2066 to U+2069, U+200E, U+200F, U+061C;
- the line and paragraph separators U+2028 and U+2029;
- zero-width characters (U+200B to U+200D, U+2060, U+FEFF) and the invisible operators (U+2061 to
  U+2064);
- the Unicode tag characters U+E0000 to U+E007F, which terminals show as nothing and so can hide
  text.

Every other character passes unchanged, including non-ASCII letters (`Café`, `日本語`) and the
variation selectors U+FE00 to U+FE0F and U+E0100 to U+E01EF, so an emoji with U+FE0F keeps its
presentation. The cost of escaping tag characters is that subdivision flag emoji show as escapes.

Page text that literally contains a backslash followed by `u{1b}` prints the same as an escaped
ESC; the two cannot be told apart in the text. The JSON output keeps the raw strings, so use it when
the difference matters.

## The end-to-end golden

`pnpm test:e2e` builds the binary, runs `spokenpath fixtures/pages/sign-in.html` and compares the
output, with the header's spokenpath version normalised to `0.0.0`, with
`fixtures/transcripts/sign-in.txt`. The golden changes only when the pinned browser is upgraded or
the text printer changes, and such a change is reviewed as drift
([ADR-0007](decisions/ADR-0007-golden-test-strategy-and-drift-reports.md)). Refresh it with
`UPDATE_FIXTURES=1 pnpm test:e2e` and review the diff. The text printer's goldens for every
committed capture are under `crates/spokenpath_cli/src/snapshots/`, including a cross-check golden
for every committed Safari WebDriver snapshot (`safari-cross-check__<page>`), a text golden for
every committed `safari-ax` snapshot (`safari-ax__<page>`), and an aligned text golden for every
committed pair (`both__<page>`), with `safari-ax__basic`, `both__basic` and `both__basic__json` for
the hand-written pair. The announce crate keeps a JSON render golden of every `safari-ax` snapshot
(`safari-ax__<page>` under `crates/spokenpath_announce/src/snapshots/`). Each test lists its
directory, so a new capture without its goldens fails.

The Safari snapshots under `fixtures/snapshots/safari-webdriver/` are captured live on a Mac, so
they are checked only by `pnpm test:safari`, which is not part of `pnpm check` or CI. It captures
every fixture page in Safari, one after another, and compares each normalised snapshot with its
committed file. Refresh them with `UPDATE_FIXTURES=1 pnpm test:safari` (with the Mac awake and
unlocked, for example under `caffeinate -i`), review the diff as drift, and refresh the
cross-check, Safari and aligned text goldens with `cargo insta test --accept -p spokenpath_cli` or
`INSTA_UPDATE=always cargo test -p spokenpath_cli`, and the Safari render goldens the same way for
`-p spokenpath_announce`. The `safari-ax` snapshots under `fixtures/snapshots/safari-ax/` are
refreshed by the same command. A live `--engine safari` run prints its golden's body, with the
header's spokenpath version normalised:

```sh
caffeinate -d -i pnpm -s spokenpath fixtures/pages/sign-in.html --engine safari > sign-in.safari.txt
diff <(sed -E '1s/^spokenpath [^ ]+ /spokenpath 0.0.0 /' sign-in.safari.txt) \
  <(sed '1,4d' crates/spokenpath_cli/src/snapshots/spokenpath_core__tests__safari-ax__sign-in.snap)
```
