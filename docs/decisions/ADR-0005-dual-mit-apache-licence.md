# ADR-0005: Dual MIT and Apache-2.0 licence

## Status

Accepted

## Date

2026-11-16

## Context

spokenpath has a Rust core and JavaScript packages, and is meant to be embedded by other tools
(linters, editors, test runners). Comparable Rust tooling is licensed MIT (oxc, Ruff) or dual MIT
and Apache-2.0 (Biome, AccessKit); dual MIT and Apache-2.0 is the Rust convention [V]
([repository hardening](../research/repo-hardening.md)).

## Decision

spokenpath is dual-licensed under the MIT licence and the Apache License, Version 2.0, at the
user's option (`LICENSE-MIT`, `LICENSE-APACHE`, `"license": "MIT OR Apache-2.0"` in
`package.json`). Unless a contributor states otherwise, any contribution intentionally submitted
for inclusion, as defined in the Apache-2.0 licence, is dual-licensed the same way, without any
additional terms or conditions.

## Consequences

- Users in either ecosystem can pick the licence their project already uses.
- Every crate and package carries the same `MIT OR Apache-2.0` licence expression.
- Dependencies under other licences are checked for compatibility before they are added.

## Rejected alternatives

- **MIT only.** Simple and common in JavaScript, but it departs from the Rust convention, and it
  has no explicit patent grant, which Apache-2.0 provides.
- **MPL-2.0.** Its file-level copyleft puts obligations on anyone who modifies and ships the core's
  files, a cost for the tools that are meant to embed it.
