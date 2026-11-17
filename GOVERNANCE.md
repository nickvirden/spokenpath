# Governance

## Maintainer

spokenpath has one maintainer, [`@nickvirden`](https://github.com/nickvirden), who has merge
authority and the final say on the project's direction.

## How changes land

- Every change, including the maintainer's own, lands through a pull request.
- `ci-ok` is the one check a pull request needs to pass, and it passes only when every other CI
  job has succeeded. The `main` branch ruleset is configured to require it once the ruleset is
  enabled.
- Pull requests are squash-merged, and the pull request title, a Conventional Commit, becomes the
  commit subject on `main` (see [CONTRIBUTING.md](CONTRIBUTING.md)).
- Code owners are listed in [.github/CODEOWNERS](.github/CODEOWNERS).

## How decisions are made

Architectural decisions are recorded as architecture decision records (ADRs) in
[docs/decisions/](docs/decisions/README.md). A change that needs a new decision adds an ADR in the
same pull request. An accepted ADR is changed only by a new ADR that supersedes it.

## Delegating approval later

While there is one maintainer, the maintainer reviews every change. Approval can be delegated
later in one of two ways:

- **Another maintainer.** A new maintainer is added to this file and to `.github/CODEOWNERS`.
- **An approval GitHub App.** A GitHub App, installed on this repository only, approves a limited
  set of changes.

`.github/CODEOWNERS` lists the owner explicitly on workflows, release configuration, the files
that run the checks and the secret scan, and this file. Those entries take effect only when the
`main` ruleset requires code-owner review. Delegating approval to a second maintainer or an App
therefore goes together with turning that requirement on, so that neither a new maintainer's
approval nor an App's approval alone can change how code is checked or released. Any change to
this file goes through a pull request like any other.
