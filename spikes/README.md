# Spikes

`spikes/` holds feasibility probes: small scripts that ask a real browser a question and record its
raw answer. They are evidence for the decision records, not product code. Nothing in `crates/` or
`packages/` imports from here, and the probes have no tests. A probe's answer is read from its
committed `output/` folder and written into an ADR; the script stays so the answer can be checked
again on a later browser build.

## Install

The probes have their own manifest and lockfile, outside the pnpm workspace, so the product's
dependencies stay free of evidence-only tooling. From the repository root:

```sh
npm ci --ignore-scripts --prefix spikes
spikes/node_modules/.bin/playwright install chromium webkit
```

The second command installs the Chromium and WebKit builds that the pinned Playwright version
expects. The Safari probes need no install: they use Safari, `safaridriver` and Swift from macOS
and Xcode, with Safari's remote automation enabled (`safaridriver --enable`, once per Mac) and, for
the accessibility tree, the Accessibility permission granted to the terminal.

## Run

Each command runs from the repository root and rewrites that probe's `output/` folder.

| Probe | Command | Needs |
|---|---|---|
| [Chromium protocol](chromium-cdp/README.md) | `node spikes/chromium-cdp/probe.mjs` | the install above |
| [Safari over WebDriver](safari-webdriver/README.md) | `node spikes/safari-webdriver/probe.mjs` | macOS, Safari remote automation |
| [Safari accessibility tree](safari-ax-tree/README.md) | `node spikes/safari-ax-tree/probe.mjs`, then `node spikes/safari-ax-tree/probe.mjs --untrusted` | macOS, remote automation, Accessibility permission, Xcode |
| [Playwright WebKit](playwright-webkit/README.md) | `node spikes/playwright-webkit/probe.mjs` | the install above |
| [VoiceOver through Guidepup](voiceover-guidepup/README.md) | `node spikes/voiceover-guidepup/check.mjs` (read-only) | macOS |

The Safari probes also run on a hosted macOS image through
[`.github/workflows/macos-probes.yml`](../.github/workflows/macos-probes.yml), which uploads their
output as an artifact. Its first run and how it compares with the local run are recorded in
[ADR-0009](../docs/decisions/ADR-0009-safari-capture-route-and-source-locations.md#hosted-runner-result).

## Committed output

The repository is public. Committed output never contains a home directory path, a user name, a
host name, a hardware serial or a URL of the file scheme. Probe pages are served over HTTP: the
Playwright probes answer requests on the fictional origin `http://spokenpath.test/` and block all
other network, and the Safari probes serve the fixture from a Node HTTP server bound to
`127.0.0.1`.
Every output file records the browser and tool versions it was produced with. Raw output is kept
byte for byte as the script wrote it, which is why Biome skips `spikes/*/output`.

## Index

| Probe | Questions | Decision record |
|---|---|---|
| [`chromium-cdp/`](chromium-cdp/README.md) | Ignored nodes, hiding ancestors, child order, text runs, name sources | [ADR-0008](../docs/decisions/ADR-0008-chromium-protocol-behaviours.md) |
| [`safari-webdriver/`](safari-webdriver/README.md) | Computed role and label per element | [ADR-0009](../docs/decisions/ADR-0009-safari-capture-route-and-source-locations.md) |
| [`safari-ax-tree/`](safari-ax-tree/README.md) | AX tree availability, trigger, permission, source-location carriers | [ADR-0009](../docs/decisions/ADR-0009-safari-capture-route-and-source-locations.md) |
| [`playwright-webkit/`](playwright-webkit/README.md) | `DOM.getAccessibilityPropertiesForNode` | [ADR-0009](../docs/decisions/ADR-0009-safari-capture-route-and-source-locations.md) |
| [`voiceover-guidepup/`](voiceover-guidepup/README.md) | Guidepup setup state; starting VoiceOver through Guidepup | [ADR-0010](../docs/decisions/ADR-0010-voiceover-capture-feasibility-and-corpus.md) |
