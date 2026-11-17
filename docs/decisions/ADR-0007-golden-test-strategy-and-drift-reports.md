# ADR-0007: Golden test strategy and drift reports

## Status

Accepted

## Date

2026-11-16

## Context

Browsers change their accessibility output between releases. If golden tests run against a live
browser, every browser upgrade either breaks CI or gets its new output approved without review,
and both hide real changes
([architecture and Chromium capture](../research/architecture-and-chromium-capture.md)). The
snapshot that capture produces is the contract between capture and the Rust core
([ADR-0002](ADR-0002-browser-ground-truth-and-rust-core.md)), so the two halves can be tested
separately.

## Decision

- **Render goldens** turn a committed snapshot into a transcript. They run in Rust, are
  deterministic and need no browser.
- **Capture goldens** turn a committed HTML fixture into a snapshot. They are tied to a pinned
  browser version, with volatile fields normalised.
- **Drift reports.** When the pinned browser version is upgraded, the difference in capture
  output is a drift report that is reviewed as part of the upgrade, not accepted automatically.
- Every snapshot and transcript records the browser version it came from.

## Consequences

- Changes to the core are tested quickly and on any machine, without a browser.
- A browser upgrade is a deliberate change with a visible, reviewed diff of what the browser
  exposes differently.
- Capture tests need the pinned browser installed.
- Committed snapshots double as fixtures for render goldens.

## Rejected alternatives

- **Goldens that run against a live, unpinned browser.** Every browser release breaks CI or is
  approved without review, so real changes in what the browser exposes go unnoticed.
- **One golden suite from HTML straight to transcript.** A failure does not say whether capture or
  interpretation changed, and every core test needs a browser.
- **Accepting capture diffs automatically on upgrade.** Drift would land unreviewed, and the
  transcript's claim to report what a named browser build exposes would rest on unchecked data.
