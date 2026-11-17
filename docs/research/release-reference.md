# Reference release setup

Read on 2026-11-16 from the public repository `nickvirden/grounded-expungement-eligibility-agent`,
whose release pipeline spokenpath's is modelled on. Flags: **[V]** read directly; **[I]** inferred.

## The reference repository

`nickvirden/grounded-expungement-eligibility-agent` (public, TypeScript). It was at v2.7.3 with a
`CHANGELOG.md`, GitHub releases, and `chore(release): x.y.z [skip ci]` commits [V].

## `.releaserc.json` [V]

```json
{
  "branches": ["main"],
  "plugins": [
    ["@semantic-release/commit-analyzer", { "preset": "conventionalcommits" }],
    ["@semantic-release/release-notes-generator", { "preset": "conventionalcommits" }],
    "@semantic-release/changelog",
    ["@semantic-release/npm", { "npmPublish": false }],
    ["@semantic-release/git", {
      "assets": ["package.json", "CHANGELOG.md"],
      "message": "chore(release): ${nextRelease.version} [skip ci]\n\n${nextRelease.notes}"
    }],
    "@semantic-release/github"
  ]
}
```

## `.github/workflows/release.yml` [V]

- Triggers on every push to `main`. semantic-release decides whether the commits since the last tag
  warrant a release; a chore-only or docs-only push produces none.
- Concurrency group without cancel-in-progress, so a publishing release is never killed mid-flight.
- Permissions: `contents`, `issues`, `pull-requests` write.
- Steps: create a GitHub App installation token with `actions/create-github-app-token` from the
  secrets `RELEASE_CLIENT_ID` and `RELEASE_APP_PRIVATE_KEY`; check out with that token and
  `fetch-depth: 0` (the checkout credential is what `git push` uses); set up Node 22;
  `npm ci --ignore-scripts --prefix .github/release`; run
  `./.github/release/node_modules/.bin/semantic-release` with `GITHUB_TOKEN` set to the App token.
- Why an App: semantic-release pushes a commit and a tag straight to `main`; with `main` protected
  by a ruleset, the default workflow token cannot bypass it, so the release runs as an App that the
  ruleset lists as a bypass actor.
- All actions are pinned to full commit SHAs.

## `.github/release/` [V]

A private `package.json` plus a committed lockfile, outside the root manifest and workspace, so
local installs do not carry release tooling and a compromised new package version cannot reach the
job until a reviewed Dependabot PR bumps the lockfile. Pinned versions read on 2026-11-16 (re-check
at install time):

- `semantic-release` 25.0.9
- `@semantic-release/changelog` 7.0.0
- `@semantic-release/git` 11.0.1
- `conventional-changelog-conventionalcommits` 8.0.0, deliberately held on 8.x: the analyzer and
  notes generator resolve the preset by name at runtime, and newer preset majors need a newer
  changelog writer than the bundled one, which throws "Missing helper" mid-release. Dependabot is
  configured to ignore major bumps of this package.

## Other pieces [V]

- **Ruleset "Protect main":** bypass actor of type `Integration` (the release App) in `always`
  mode; deletion and non-fast-forward blocked; pull request required with 0 approvals; required
  status checks.
- **Merge settings:** squash, merge and rebase all allowed; merge commits are what is used, so each
  Conventional Commit reaches `main` individually.
- **Commit messages are not linted** anywhere in that repository; the convention is followed, not
  enforced.
- **`.husky/pre-commit`** runs `gitleaks protect --staged --config .gitleaks.toml` and warns if
  gitleaks is not installed. `.gitleaks.toml` extends the default rules and allowlists
  `.env.example` and lockfiles.
- **Dependabot** covers `github-actions` and the npm manifest in `/.github/release`, weekly,
  grouped, with the commit prefix `ci`.

## What differs for a public, multi-language project [I]

- **Versions start at 1.0.0 by default.** With no prior tag, semantic-release publishes the first
  releasable commit as 1.0.0. A seed tag (for example `v0.0.0`) makes the first `feat` 0.1.0, and a
  commit-analyzer `releaseRules` entry mapping breaking changes to a minor bump keeps the project in
  0.x until 1.0 is declared deliberately.
- **Squash merging** makes the PR title the commit subject on `main`, so linting the PR title in CI
  enforces Conventional Commits even for outside contributors.
- **Several manifests.** A Rust workspace and npm packages need one version written to
  `Cargo.toml`, `Cargo.lock` and each `package.json`, which the stock plugins do not do.
- **Release commits pushed by an App with git are unsigned**, which matters if signed commits are
  later required on `main`.
