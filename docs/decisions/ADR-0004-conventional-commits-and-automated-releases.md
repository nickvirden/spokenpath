# ADR-0004: Conventional Commits and automated releases

## Status

Accepted

## Date

2026-11-16

## Context

Commit conventions and version tags are in place before any product code, so every change that
contributes to the product is described and versioned the same way. The release pipeline follows
an existing, working semantic-release setup
([release reference](../research/release-reference.md)). That setup follows the convention without
enforcing it, which does not hold up in a public project that receives commits from people who do
not know the convention.

With no prior tag, semantic-release publishes the first releasable commit as 1.0.0, which would
contradict a status of pre-alpha. The project will hold Rust crates and npm packages in one
repository. `main` is protected, and the default workflow token cannot bypass that protection to
push a release commit and tag.

## Decision

- **Conventional Commits, enforced.** A commitlint `commit-msg` hook checks every commit locally,
  and a CI check (`pr-title`) lints the pull request title with the same rules
  (`commitlint.config.mjs`). The check runs on `pull_request` and reads the title from an
  environment variable.
- **Squash merges.** Pull requests are squash-merged, and the pull request title becomes the commit
  subject on `main`, so every commit on `main` is a linted Conventional Commit.
- **semantic-release on every push to `main`** (`.github/workflows/release.yml`, configured in
  `.releaserc.json`) with the `conventionalcommits` preset. When the commits since the last tag
  warrant a release, it writes `CHANGELOG.md`, bumps the version in `package.json`, pushes a
  `chore(release): x.y.z [skip ci]` commit, tags it `vX.Y.Z` and creates the GitHub release. When
  they do not, it does nothing. Nothing is published to npm (`npmPublish: false`).
- **Versions start at 0.x.** A seed tag `v0.0.0` sits on the initial commit, so the first `feat`
  releases 0.1.0. A release rule in `.releaserc.json` maps breaking changes to a minor bump, which
  keeps the project in 0.x until 1.0 is declared on purpose by changing that rule.
- **Pinned release tooling.** The release tooling is pinned in `.github/release/package.json` with
  a committed lockfile, outside the root manifest, and installed with
  `npm ci --ignore-scripts --prefix .github/release`. A test checks that the release rules compute
  the expected versions.
- **One version for the whole repository.** Crates and npm packages move in lockstep. The release
  commit bumps the root `package.json`; once the Cargo and pnpm workspaces exist, the release step
  also bumps `Cargo.toml`, `Cargo.lock` and every package manifest to the same version.
- **A dedicated release App.** Releases run as a GitHub App, `spokenpath-release`, installed on
  this repository only. It is the only actor allowed to push to `main` directly and to create `v*`
  tags. Its credentials live in the repository secrets `RELEASE_CLIENT_ID` and
  `RELEASE_APP_PRIVATE_KEY`.

## Consequences

- Nobody pushes `v*` tags, edits `CHANGELOG.md` or bumps a version by hand.
- `docs`, `chore`, `ci` and other non-releasing types merge without cutting a release; `feat`
  cuts a minor release and `fix` or `perf` a patch while the version is 0.x.
- `conventional-changelog-conventionalcommits` stays on 8.x, because newer majors need a newer
  changelog writer than the one semantic-release bundles and fail mid-release.
- One tag describes one coherent state of every crate and package.
- Release commits pushed by the App are unsigned, which matters if signed commits are later
  required on `main`.
- Extending the release step to the Cargo and package manifests is work that lands with the
  workspaces.

## Rejected alternatives

- **Following the convention without enforcement.** Outside contributors do not know it, so
  `main` would collect non-conforming commits and semantic-release would misjudge releases.
- **Merge commits instead of squash merges.** Every commit on a branch would reach `main` and would
  all need to conform; squash merging lets one linted title stand for the change.
- **A Rust-specific release tool (release-plz).** The semantic-release pattern is already proven
  in a working pipeline, and one tool covering both ecosystems keeps releases simple for a solo
  maintainer.
- **A third-party pull request title action running on `pull_request_target`.** That trigger runs
  with a privileged token in the context of the base branch, which is the wrong exposure for
  attacker-controlled title text.
- **Starting at 1.0.0.** semantic-release's default would publish the first `feat` as 1.0.0,
  contradicting a pre-alpha status.
- **Independent per-package versions.** More release machinery, and no single tag that describes
  the state of the whole repository.
- **Reusing another repository's release App.** A public project would share a push identity with
  another repository, so a leaked key would reach both.
- **Pushing release commits with the default workflow token.** It cannot bypass the protection on
  `main`.
