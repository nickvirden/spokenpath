# Contributing to spokenpath

Thank you for helping. spokenpath is pre-alpha, so the most useful contributions right now are
reports where its output disagrees with a real browser or screen reader, and discussion of the
[roadmap](docs/roadmap.md) and the [decision records](docs/decisions/README.md).

Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md). Report security problems
privately, as described in the [security policy](SECURITY.md), never in a public issue.

[AGENTS.md](AGENTS.md) is the full reference for working in a checkout. This page summarises it;
where the two differ, AGENTS.md is right.

## Setup

Install Node at the version in `.nvmrc` and enable pnpm with `corepack enable`, then install the
root dev tooling (which also installs the git hooks) and the pinned release tooling:

```sh
pnpm install
npm ci --ignore-scripts --prefix .github/release
```

The checks also call native tools (gitleaks, actionlint, shellcheck and zizmor). See
[Setup in AGENTS.md](AGENTS.md#setup) for the exact versions and how to install each one.

## Checks

Run every check before opening a pull request:

```sh
git fetch origin
pnpm check
```

`pnpm check` runs the checks in order and stops at the first failure. The
[checks table in AGENTS.md](AGENTS.md#checks) lists each one with its own command. CI runs the same
checks on every pull request, plus a lint of the pull request title.

## Commits and pull request titles

Every commit message and every pull request title is a
[Conventional Commit](https://www.conventionalcommits.org/): `<type>(<scope>): <description>`, with
an optional scope. The allowed types are `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`,
`refactor`, `revert`, `style` and `test`.

The `commit-msg` hook rejects a commit whose message does not follow this. Pull requests are
squash-merged, and the pull request title becomes the commit subject on `main`, so CI lints the
title with the same rules.

While the version is 0.x:

- `feat` cuts a minor release (0.1.0 to 0.2.0);
- `fix` and `perf` cut a patch release (0.1.0 to 0.1.1);
- a breaking change (`feat!:`, `fix!:`, or a `BREAKING CHANGE:` footer) cuts a minor release;
- a GitHub-style revert cuts a patch release;
- every other type cuts no release.

## How releases work

Releases are automated. Every push to `main` runs semantic-release, which decides from the commit
types since the last tag whether to release. When it releases, it writes `CHANGELOG.md`, bumps the
version, commits, tags `vX.Y.Z` and creates the GitHub release. One version covers the whole
repository.

Never do any of these by hand:

- push a `v*` tag;
- edit `CHANGELOG.md`;
- bump a version in any manifest.

[How releases work in AGENTS.md](AGENTS.md#how-releases-work) and
[ADR-0004](docs/decisions/ADR-0004-conventional-commits-and-automated-releases.md) have the details.

## How decisions are recorded

Architectural decisions are recorded as ADRs in `docs/decisions/`, in the format that
[the decision records README](docs/decisions/README.md) defines. Read the ADRs that cover an area
before changing it. A pull request that makes or changes a decision adds an ADR, or a new ADR that
supersedes an old one, in the same pull request.

## Wording for anything user-facing

spokenpath shows what the browser exposes. In output, docs, and messages:

- Say "what the browser exposes" for the accessibility data, and "illustrative speech,
  approximate" for any spoken phrasing.
- Never claim to reproduce what a screen reader says or what users experience.
- Make no conformance claims (WCAG or otherwise).

## Licence of contributions

spokenpath is dual-licensed under the [MIT licence](LICENSE-MIT) and the
[Apache License, Version 2.0](LICENSE-APACHE). Unless you explicitly state otherwise, any
contribution intentionally submitted for inclusion in the work by you, as defined in the
Apache-2.0 licence, shall be dual licensed as above, without any additional terms or conditions.
