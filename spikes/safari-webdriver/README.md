# Safari over WebDriver

What real Safari answers to WebDriver's Get Computed Role and Get Computed Label for each element
of a tagged fixture, and how long each call takes. The answers are recorded in
[ADR-0009](../../docs/decisions/ADR-0009-safari-capture-route-and-source-locations.md).

## Questions

- Does a `safaridriver` session start, and does Safari answer both calls for every element?
- Which element types error or return an empty role or label?
- How long does a call take, and the whole session?

## Run

```sh
node spikes/safari-webdriver/probe.mjs; echo exit=$?; pgrep -x safaridriver || echo stopped
```

[`probe.mjs`](probe.mjs) uses only Node built-ins ([`webdriver.mjs`](webdriver.mjs) is its client).
It starts `safaridriver` on a free port, serves [`fixture/tagged.html`](fixture/tagged.html) from a
Node HTTP server on `127.0.0.1`, opens a session, finds every element carrying
`data-spokenpath-src`, and records per element the attribute value, the tag, Get Computed Role
(`GET /session/{id}/element/{eid}/computedrole`), Get Computed Label
(`GET /session/{id}/element/{eid}/computedlabel`), any WebDriver error, and the time per call. It
always deletes the session and stops `safaridriver`, including on error. It exits 0 once the
session started (`sessionStarted: true` in the output), even when single elements error, and 1
only when `safaridriver` or the session cannot start.

The fixture has 24 tagged elements. Each `data-spokenpath-src` value is the element's real start
tag position in the fixture, `tagged.html:<line>:<column>`. The
[accessibility tree probe](../safari-ax-tree/README.md) loads the same fixture.

| | Version |
|---|---|
| macOS | macOS 27.0.1 (26A434) |
| Safari | 27.0.1 (22625.1.29.11.28), and `browserVersion` 27.0.1 from the session |
| Chromium | not used |
| Playwright | not used |
| Node | 24.13.0 |
| Date run | 2026-11-17 |

## Results

| Question | Result | Evidence |
|---|---|---|
| A session starts and both calls answer | **pass**: the session started, 23 of 24 elements answered both calls, the session was deleted and `safaridriver` stopped | [`webdriver.json`](output/webdriver.json) |
| Element types that error or return empty | recorded below | [`webdriver.json`](output/webdriver.json) (`erroredOrEmpty`) |
| Timing | **pass**: every call took 1.8 to 31.2 ms; the whole run, including starting `safaridriver` and Safari, took 1260.8 ms | [`webdriver.json`](output/webdriver.json) (`roleMs`, `labelMs`, `totalMs`) |

### Per element

| `data-spokenpath-src` | Tag | Computed role | Computed label | ms (role / label) |
|---|---|---|---|---|
| `tagged.html:12:1` | `nav` | `navigation` | "Site" | 2 / 2.9 |
| `tagged.html:13:3` | `a` | `link` | "Next page" | 7 / 2.1 |
| `tagged.html:15:1` | `main` | `main` | (empty) | 2.7 / 2.1 |
| `tagged.html:16:3` | `h1` | `heading` | "Tagged fixture" | 3.6 / 8.9 |
| `tagged.html:17:3` | `h2` | `heading` | "Form controls" | 2.4 / 10.4 |
| `tagged.html:18:3` | `p` | `paragraph` | (empty) | 2.4 / 2.9 |
| `tagged.html:19:3` | `p` | `paragraph` | (empty) | 2.2 / 2 |
| `tagged.html:20:3` | `button` | `button` | "Save" | 2.6 / 1.8 |
| `tagged.html:21:3` | `label` | (empty) | (empty) | 1.9 / 1.8 |
| `tagged.html:22:3` | `input` | `textbox` | "Name" | 1.8 / 2.2 |
| `tagged.html:23:10` | `input` | `checkbox` | "Subscribe" | 2 / 2.1 |
| `tagged.html:24:10` | `input` | `radio` | "Small" | 1.8 / 1.9 |
| `tagged.html:26:5` | `select` | `button` | "Flavour" | 1.8 / 2 |
| `tagged.html:27:7` | `optgroup` | error | error | 2 / 2.3 |
| `tagged.html:33:19` | `textarea` | `textbox` | "Comments" | 1.9 / 2.1 |
| `tagged.html:34:3` | `img` | (empty) | "Grey square" | 2 / 2.3 |
| `tagged.html:36:5` | `area` | `link` | "Corner" | 3.5 / 31.2 |
| `tagged.html:38:3` | `ul` | `group` | (empty) | 2.5 / 2.3 |
| `tagged.html:39:5` | `li` | `listitem` | (empty) | 2.2 / 3.1 |
| `tagged.html:42:3` | `table` | `table` | "Prices" | 2.1 / 2.3 |
| `tagged.html:47:3` | `section` | `region` | "Labelled region" | 4.5 / 2 |
| `tagged.html:51:3` | `input` | `searchbox` | "Search the site" | 2 / 2 |
| `tagged.html:52:3` | `div` | `generic` | (empty) | 2.1 / 2 |
| `tagged.html:53:3` | `dialog` | `dialog` | "Notice" | 2.1 / 2.1 |

### Errors and empty answers

- `optgroup` (`tagged.html:27:7`): both calls fail with the raw error
  `{"error": "unknown error", "message": "", "stacktrace": ""}`.
- `img` with `usemap` (`tagged.html:34:3`): empty role; the label is the `alt` text.
- `label` (`tagged.html:21:3`): empty role and empty label.
- `ul` with `list-style: none` (`tagged.html:38:3`): role `group`, not `list`, while its `li`
  reports `listitem`.
- `select` (`tagged.html:26:5`): role `button`, not `combobox`.
- `div` with `aria-hidden="true"` (`tagged.html:52:3`): role `generic` and an empty label; nothing
  in either answer says the element is hidden.
- Empty labels with a role: `main`, both `p`, `ul` and `li`, which have no name.

## Hosted run

The [`macos-probes` workflow](../../.github/workflows/macos-probes.yml) ran this probe once on a
hosted `macos-26` image (image version 20261102.0473.1, macOS 26.7.2, Safari 27.2):
[run 44191210745](https://github.com/nickvirden/spokenpath/actions/runs/44191210745). The session
started, 23 of 24 elements answered both calls, `optgroup` failed with the same `unknown error`,
and every role and label matched the local run except the `ul` with `list-style: none`, which
reported `list` instead of `group`. Calls took 2.2 to 158.3 ms and the whole run 15930.8 ms. Its
output is in the run's artifact, not committed; the comparison is in
[ADR-0009](../../docs/decisions/ADR-0009-safari-capture-route-and-source-locations.md#hosted-runner-result).
