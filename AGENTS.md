# AGENTS.md

Instructions for anyone, human or agent, working in a checkout of spokenpath. Follow them in order
on a fresh clone.

## Setup

1. Install Node at the version in `.nvmrc` (Node 24), for example with `nvm install` then
   `nvm use` in the repository root. Use 24.15 or later: older 24.x releases install the release
   tooling with `EBADENGINE` warnings.
2. Enable pnpm at the version pinned in `package.json` (`packageManager`):

   ```sh
   corepack enable
   ```

   If corepack is not available, install the same version directly:
   `npm install --global pnpm@10.33.0`.
3. Install the root dev tooling. This also installs the git hooks (through the `prepare` script):

   ```sh
   pnpm install
   ```

4. Install the pinned release tooling, which the release-rule test imports:

   ```sh
   npm ci --ignore-scripts --prefix .github/release
   ```

5. Install the native tools the checks call. A commit works without them (the secret-scan hook
   warns and skips when gitleaks is missing), but `pnpm check` needs all four:

   | Tool | Used by | Install (macOS) | Elsewhere |
   |---|---|---|---|
   | gitleaks | `pre-commit` hook | `brew install gitleaks` | https://github.com/gitleaks/gitleaks/releases |
   | actionlint | `pnpm lint:workflows` | `brew install actionlint` | https://github.com/rhysd/actionlint/releases |
   | shellcheck | `pnpm lint:hooks`, and actionlint's checks of `run:` blocks | `brew install shellcheck` | `apt-get install shellcheck` |
   | zizmor | `pnpm zizmor` | `brew install zizmor` | `pipx install zizmor` |

## Checks

Run them in this order. Each is also a step of `pnpm check`, which runs all of them in this order
and stops at the first failure:

| Order | Check | Command |
|---|---|---|
| 1 | Commit messages on this branch are Conventional Commits | `pnpm lint:commits` |
| 2 | JSON configuration parses | `pnpm lint:json` |
| 3 | Git hooks pass shellcheck | `pnpm lint:hooks` |
| 4 | Workflows pass actionlint | `pnpm lint:workflows` |
| 5 | Workflows and Dependabot config pass zizmor | `pnpm zizmor` |
| 6 | Release rules compute the expected versions | `pnpm test:release` |
| all | Everything above | `pnpm check` |

`pnpm lint:commits` (`scripts/lint-commits.mjs`) checks the commits between `origin/main` and
`HEAD`, so fetch first (`git fetch origin`) if `origin/main` is stale. When there are no commits in
that range, as on `main` itself, it passes without linting anything. When `origin/main` does not
exist it fails. `pnpm test:release` needs step 4 of Setup.

CI runs the same checks on every pull request, plus a lint of the pull request title.

## Commits and pull request titles

Every commit message and every pull request title is a
[Conventional Commit](https://www.conventionalcommits.org/): `<type>(<scope>): <description>`,
with an optional scope. The allowed types are:

`build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `revert`, `style`, `test`.

The `commit-msg` hook rejects a commit whose message does not follow this. Pull requests are
squash-merged, and the pull request title becomes the commit subject on `main`, so CI lints the
title with the same rules (`commitlint.config.mjs`).

Which types cut a release:

| Commit | Release while the version is 0.x |
|---|---|
| `feat` | minor (0.1.0 to 0.2.0) |
| `fix`, `perf` | patch (0.1.0 to 0.1.1) |
| A breaking change (`feat!:`, `fix!:`, or a `BREAKING CHANGE:` footer) | minor |
| Every other type | none |
| A GitHub-style revert (`Revert "..."` subject, `This reverts commit ...` body) | patch |

commitlint ignores GitHub's revert titles rather than linting them, and semantic-release releases
them as a patch, so squash-merging GitHub's "Revert" pull request cuts a patch release.

Breaking changes map to minor through a release rule in `.releaserc.json`, which keeps the
project in 0.x. Leaving 0.x is a deliberate change to that rule, never a side effect of a commit.

## How releases work

Every push to `main` runs semantic-release (`.github/workflows/release.yml`, configured in
`.releaserc.json`). When the commits since the last tag warrant a release, it writes
`CHANGELOG.md`, bumps the version in `package.json`, pushes a
`chore(release): x.y.z [skip ci]` commit, tags it `vX.Y.Z` and creates the GitHub release. When
they do not, it does nothing. Releases run as the repository's release GitHub App, the only actor
allowed to push to `main` directly and to create `v*` tags.

Never do any of these by hand:

- push a `v*` tag;
- edit `CHANGELOG.md`;
- bump a version in any manifest.

The release tooling is pinned in `.github/release/package.json` with a committed lockfile.
`conventional-changelog-conventionalcommits` stays on 8.x: newer majors need a newer changelog
writer than the one semantic-release bundles and fail mid-release.

## Issues and decisions

- Issues are tracked in GitHub Issues on this repository.
- Architectural decisions are recorded as ADRs in `docs/decisions/`.

## Comments

Comments explain why, in timeless present tense. Never reference plans, phases, review rounds or
task numbers in comments, test names, error messages or docs; state the constraint or failure mode
itself.

## Wording for anything user-facing

spokenpath shows what the browser exposes. In output, docs, and messages:

- Say "what the browser exposes" for the accessibility data, and "illustrative speech,
  approximate" for any spoken phrasing.
- Never claim to reproduce what a screen reader says or what users experience.
- Make no conformance claims (WCAG or otherwise).

## The required check

`ci-ok` in `.github/workflows/ci.yml` is the one required status check. It passes only when every
other CI job succeeded. Its job id and name must stay `ci-ok`; add any new CI job to its `needs`.
