# Safari's macOS accessibility tree

What a native program reads from Safari's macOS accessibility (AX) tree for the tagged fixture,
and which carrier can take a source location from the page to an AX node. The answers are
recorded in [ADR-0009](../../docs/decisions/ADR-0009-safari-capture-route-and-source-locations.md).

## Questions

- Is the AX tree under Safari's `AXWebArea` populated on the first query, or does it need a
  trigger (a repeated query, a delay, or setting `AXManualAccessibility` or
  `AXEnhancedUserInterface` on the Safari application element)?
- Which attributes does each node report, and what do `AXDOMIdentifier` and `AXDOMClassList`
  contain?
- Which carrier maps each tagged element to exactly one AX node: (i) a class token derived from
  `data-spokenpath-src`, read back through `AXDOMClassList`; (ii) the element `id`, read back
  through `AXDOMIdentifier`; (iii) matching by document position against the WebDriver probe's
  label sequence? Does any AX attribute expose `data-*` attributes directly?
- What does the program read when it does not hold the Accessibility permission
  (`AXIsProcessTrusted()` is false)?

## Run

```sh
node spikes/safari-ax-tree/probe.mjs; echo exit=$?; pgrep -x safaridriver || echo stopped
node spikes/safari-ax-tree/probe.mjs --untrusted; echo exit=$?
launchctl list | grep spokenpath || echo no-launchd-job
```

[`probe.mjs`](probe.mjs) drives Safari through a `safaridriver` session (the automation window),
not `open -a Safari`. It serves the
[WebDriver probe's fixture](../safari-webdriver/fixture/tagged.html) from `127.0.0.1` in three
variants, spliced into the served text next to each `data-spokenpath-src` attribute:

- `tagged-plain.html`: the fixture unchanged;
- `tagged-class.html`: a class token `sp-src-<line>-<column>` added to each tagged element (joined
  to an existing `class` value where the element has one);
- `tagged-id.html`: an `id="sp-src-<line>-<column>"` added to each tagged element. An element
  that already has an author `id` keeps it, because the HTML parser keeps the first of two
  duplicate attributes.

For each variant it navigates the session, waits 500 ms, and runs
[`ax-tree.swift`](ax-tree.swift) with `swift` (no build step, no Xcode project). The Swift program
records `AXIsProcessTrusted()`, finds the `AXWebArea` whose `AXURL` contains the variant's path
(other Safari windows are passed over unread), and records for every node under it: role,
subrole, role description, title, description, value, help, `AXDOMIdentifier`, `AXDOMClassList`,
the full `AXUIElementCopyAttributeNames` list, and the names of any string attributes whose value
contains `tagged.html:` (a `data-spokenpath-src` value). If the first query returns one node or
none, it retries after 1 s, then after setting `AXManualAccessibility`, then
`AXEnhancedUserInterface` on the Safari application element, and puts each back afterwards. It
changes no system setting. Every attempt records the AX error code of the first call and its
name. When `AXIsProcessTrusted()` is false or that call fails with `kAXErrorAPIDisabled`
(`-25211`), the program records `permission: "missing"`, stops retrying, and exits 3, so callers
can tell that case apart from an empty page; `probe.mjs` passes that status on. Each Swift run has
a 60 s deadline, after which the child is killed and the run error is recorded; `safaridriver`
and the session are cleaned up either way.

#### The untrusted run

`--untrusted` answers what the helper reads without the permission, without changing any
setting. The terminal holds the Accessibility permission and every child of it inherits that, so
the run leaves the terminal's process tree instead:

1. It opens a `safaridriver` session showing the plain fixture, so there is a web area to find.
2. It builds `ax-tree.swift` with `swiftc` into a new temporary folder (`mkdtemp` under
   `$TMPDIR`), starting every program (`swiftc`, `launchctl`) from an argument array with no
   shell.
3. It submits the binary as a transient launchd job of the invoking user
   (`launchctl submit -l test.spokenpath.ax-tree-untrusted ...`), never with `sudo`. The job's
   standard output and error go to files in the temporary folder. A binary started by launchd is
   its own responsible process, and it has never been granted the permission.
4. As soon as the job has printed its JSON, the probe reads launchd's `LastExitStatus` and runs
   `launchctl remove`; it removes the job again in a `finally` on error, timeout or signal, and
   deletes the temporary folder. `launchctl submit` restarts a job that exits, which is why the
   job is removed rather than left to finish.

The committed [`ax-tree-untrusted.json`](output/ax-tree-untrusted.json) holds the program's JSON,
the label and the exit status, and no path.

| | Version |
|---|---|
| macOS | macOS 27.0.1 (26A434) |
| Safari | 27.0.1 (22625.1.29.11.28) |
| Chromium | not used |
| Playwright | not used |
| Swift | 6.4.2 (Xcode) |
| Node | 24.13.0 |
| Date run | 2026-11-17 |

## Results

| Question | Result | Evidence |
|---|---|---|
| AX tree populated on the first query | **pass**: 72 nodes on the first query in every variant; no trigger needed and none set | [`ax-tree-plain.json`](output/ax-tree-plain.json) (`attempts`) |
| Attributes, `AXDOMIdentifier`, `AXDOMClassList` | **pass**: every node reports both; 87 distinct attribute names across the tree | [`ax-tree-plain.json`](output/ax-tree-plain.json) |
| Carrier (i): class token through `AXDOMClassList` | **pass**: 22 of 24 tagged elements map to exactly one node | [`carriers.json`](output/carriers.json), [`ax-tree-class.json`](output/ax-tree-class.json) |
| Carrier (ii): element `id` through `AXDOMIdentifier` | **fail**: 21 of 24; an author `id` displaces it | [`carriers.json`](output/carriers.json), [`ax-tree-id.json`](output/ax-tree-id.json) |
| Carrier (iii): position against WebDriver labels | **fail**: 16 of 24; elements with an empty label cannot be matched | [`carriers.json`](output/carriers.json) |
| `data-*` exposed directly by any attribute | **fail**: no string attribute of any node contains a `data-spokenpath-src` value | [`carriers.json`](output/carriers.json) (`directDataAttributeExposure: []`) |
| Without the permission | **pass**: the untrusted run reads `axIsProcessTrusted: false` and the first call fails with `-25211` (`kAXErrorAPIDisabled`): no web area seen, 0 nodes, `permission: "missing"`, exit status 3. The tree is empty with an error code, not silently | [`ax-tree-untrusted.json`](output/ax-tree-untrusted.json) |

### Details

- `AXDOMIdentifier` is the element's `id` (the empty string when it has none); in the plain
  variant only node 16, `input#name`, has one (`name`). `AXDOMClassList` is the element's class
  list as an array; in the plain variant only node 11 has one (`["note"]`). Every
  `AXStaticText` node reports an empty identifier and an empty class list.
- Class token misses: `tagged.html:27:7` (`optgroup`, inside the closed `select` popup, which has
  no AX children) and `tagged.html:52:3` (the `aria-hidden="true"` `div`, which has no AX node).
  Every tagged element that has an AX node maps, including the `p` with an author class (node 11,
  `["note", "sp-src-19-3"]`).
- `id` misses: the same two, plus `tagged.html:22:3`, whose author `id="name"` wins over the
  spliced one.
- Position misses: the eight elements whose WebDriver label is empty (`main`, both `p`, `label`,
  `optgroup`, `ul`, `li`, the hidden `div`). All 16 positional matches land on the node that holds
  the same element's class token in the class variant (`matchesConfirmedByClassToken: 16`).
- One local run of the trusted mode hung reading the web area: the Swift program had not
  finished after 60 s, so the deadline killed it and recorded
  `swift did not finish within 60000 ms`, and `safaridriver` was stopped. The run before and
  every run after completed in about 5 s. A capture helper needs a deadline on every AX read.
- The `ul` with `list-style: none` is `AXGroup` with subrole `AXApplicationGroup`; the `img` with
  `usemap` is `AXImageMap`; the `select` is `AXPopUpButton`.

## Hosted run

The [`macos-probes` workflow](../../.github/workflows/macos-probes.yml) ran the trusted mode once
on a hosted `macos-26` image (image version 20261102.0473.1, macOS 26.7.2, Safari 27.2):
[run 44191210745](https://github.com/nickvirden/spokenpath/actions/runs/44191210745). The helper
was trusted for Accessibility there (`AXIsProcessTrusted()` true, exit status 0). Each variant gave
72 nodes whose role, subrole, title and description match the local run; the plain variant's first
query found no web area yet and the repeat after 1 s found it, with no attribute set. The carriers
mapped 22, 21 and 16 of 24, as locally. The workflow does not run `--untrusted`, so the hosted run
says nothing about the no-permission path. Its output is in the run's artifact, not committed; the
comparison is in
[ADR-0009](../../docs/decisions/ADR-0009-safari-capture-route-and-source-locations.md#hosted-runner-result).
