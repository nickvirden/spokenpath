# Architecture decision records

An architecture decision record (ADR) states one decision that shapes spokenpath, why it was made,
and which alternatives lost and why. Read the ADRs that cover an area before changing it, and cite
them instead of deciding the same question again.

## When to write one

Write an ADR when a change:

- sets or changes the product's output, its honesty rules or its claims;
- chooses or replaces a language, a browser engine, a capture route or a major dependency;
- changes the snapshot schema's contract, the release process or the licence;
- puts something in or out of scope.

A change that follows an existing ADR does not need a new one.

## Format

Each ADR is one Markdown file in this folder named `ADR-NNNN-<kebab-slug>.md`, where `NNNN` is the
next free four-digit number. The file has one H1, `ADR-NNNN: <title>`, followed by these sections
in this order:

- `## Status`: Proposed, Accepted, or Superseded by ADR-NNNN.
- `## Date`: the date of the decision, as `YYYY-MM-DD`.
- `## Context`: the problem, the constraints and the evidence, with links to `docs/research/`.
- `## Decision`: what is decided, stated so a later change can be checked against it.
- `## Consequences`: what follows, good and bad, including the work it creates.
- `## Rejected alternatives`: each alternative and the reason it loses.

Write in timeless present tense. An ADR records a decision, not the history of a discussion.

## Changing a decision

An accepted ADR is not rewritten. To change a decision, add a new ADR that states the new decision
and names the one it supersedes, then change the old ADR's status to `Superseded by ADR-NNNN` and
add the new ADR to the index. Fixing a typo or a broken link in an accepted ADR is fine.

## Index

| ADR | Title | Status |
|---|---|---|
| [ADR-0001](ADR-0001-two-layer-output.md) | Two-layer output: what the browser exposes, and illustrative speech | Accepted |
| [ADR-0002](ADR-0002-browser-ground-truth-and-rust-core.md) | Real browsers are ground truth; a Rust core does all interpretation | Accepted |
| [ADR-0003](ADR-0003-source-tagging-by-splicing.md) | Source tagging by splicing attributes into the original text | Accepted |
| [ADR-0004](ADR-0004-conventional-commits-and-automated-releases.md) | Conventional Commits and automated releases | Accepted |
| [ADR-0005](ADR-0005-dual-mit-apache-licence.md) | Dual MIT and Apache-2.0 licence | Accepted |
| [ADR-0006](ADR-0006-engine-scope.md) | Engine scope: Chromium first, then Safari and VoiceOver on macOS | Accepted |
| [ADR-0007](ADR-0007-golden-test-strategy-and-drift-reports.md) | Golden test strategy and drift reports | Accepted |
| [ADR-0008](ADR-0008-chromium-protocol-behaviours.md) | Chromium protocol behaviours the model must handle | Accepted |
| [ADR-0009](ADR-0009-safari-capture-route-and-source-locations.md) | Safari capture route and how source locations travel through Safari | Accepted |
| [ADR-0010](ADR-0010-voiceover-capture-feasibility-and-corpus.md) | VoiceOver capture feasibility and the speech corpus | Accepted |
