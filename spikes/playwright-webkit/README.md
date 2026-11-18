# Playwright WebKit

Whether Playwright's bundled WebKit build answers the Web Inspector call
`DOM.getAccessibilityPropertiesForNode`. The answer is recorded in
[ADR-0009](../../docs/decisions/ADR-0009-safari-capture-route-and-source-locations.md).

## Question

- Does the WebKit build answer `DOM.getAccessibilityPropertiesForNode` on element nodes, and with
  what?

## Run

```sh
npm ci --ignore-scripts --prefix spikes
spikes/node_modules/.bin/playwright install chromium webkit
node spikes/playwright-webkit/probe.mjs
```

[`probe.mjs`](probe.mjs) launches Playwright's bundled WebKit headless and loads
[`pages/accessibility-properties.html`](pages/accessibility-properties.html) from the fictional
origin `http://spokenpath.test/`, with every other request aborted. Playwright's public API has no
protocol session for WebKit, so the script reaches the page's inspector session through Playwright
internals: `page._connection.toImpl(page).delegate._session`. **This is unsupported API**: it works
only with Playwright's in-process connection and may change in any Playwright release. Through that
session it calls `DOM.getDocument`, `DOM.querySelectorAll` for `[data-probe]`, and
`DOM.getAccessibilityPropertiesForNode` on each of the nine element nodes, recording the raw
response or the raw error.

| | Version |
|---|---|
| macOS | macOS 27.0.1 (26A434) |
| Safari | not used |
| Chromium | not used |
| WebKit | 27.2, as reported by `browser.version()` |
| Playwright | 1.65.1 |
| Node | 24.13.0 |
| Date run | 2026-11-17 |

## Results

| Question | Result | Evidence |
|---|---|---|
| `DOM.getAccessibilityPropertiesForNode` answers | **pass**: 9 of 9 element nodes answered | [`accessibility-properties.json`](output/accessibility-properties.json) |

Each response has `properties` with `exists`, `role`, `label`, `nodeId`, `parentNodeId` and
`childNodeIds`, plus state fields where they apply (`focused`, `required`, `headingLevel`). The
`aria-hidden` button reports `ignored: true`, `ignoredByDefault: true` and `hidden: true` while
keeping its role (`button`) and label; the `display:none` button reports `exists: false` with an
empty role and label. The unstyled `ul` reports role `group`. The response carries no name
provenance and no reason for exclusion beyond those flags.
