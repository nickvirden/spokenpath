# Repository hardening for a solo-maintainer public repository on a personal GitHub account

Research date: 2026-11-16. Method: GitHub documentation fetched by a research agent, plus direct
API calls against this repository. GitHub features change often; confirm against current
documentation when applying.

Flags: **[V]** a source was fetched or an API call observed it; **[I]** inferred; **[U]** not confirmed.

## Findings that constrain the setup

- **Rulesets need a public repository or GitHub Pro on a personal account.** Observed directly: the
  rulesets API on this repository, while it was private, returned "Upgrade to GitHub Pro or make
  this repository public to enable this feature" [V].
- **Rulesets are preferred over classic branch protection.** They layer (most restrictive wins),
  can be disabled without deletion, are visible to readers, and can target tags [V].
  https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/about-rulesets
- **An author cannot approve their own pull request.** With one maintainer and a required approval,
  the maintainer's own PRs cannot merge without a bypass or a second approver [V/I].
- **Bypass modes** are `always`, `pull_request` and `exempt` [V]. Required reviewers by team are not
  available on user-owned repositories [V]. `evaluate` enforcement is Enterprise only [V].
- **The GitHub CLI token used during the research** returned "Resource not accessible by
  integration" on protection endpoints, so some settings may need the web UI [V].
- **Signed commits:** unsigned commits in a PR branch can block a squash merge even though the
  final commit would be signed [V]. Enable only after signing is set up.

## The solo-maintainer options

| Option | Effect | Trade-off |
|---|---|---|
| Required approval with an admin bypass in `pull_request` mode | Owner merges own PRs through an audited bypass; direct pushes stay blocked | Owner can bypass, which is acceptable for a solo owner |
| No approval count; PR and status checks only | Everything needs a PR and green CI | Functionally equivalent while only the owner has write access; must be tightened when a collaborator or bot is added |
| A bot or second account as approver | Bot approves the owner's PRs | Appearance of review without an independent reviewer; letting Actions approve PRs is flagged as a risk in GitHub's docs [V] |
| Merge queue | Serializes merges | Does not solve approval; availability on personal repositories not confirmed [U] |

Automation that merges without admin override cannot satisfy a required approval the author is
unable to give, so an approval count stalls automated merges until a real second approver exists.

To delegate approval to a bot later: create a GitHub App (not a user token), install it on this
repository only, restrict what it may approve, keep the owner's review for workflows, rulesets and
release configuration through CODEOWNERS [I].

## Ruleset API

`POST repos/{owner}/{repo}/rulesets` [V]. https://docs.github.com/en/rest/repos/rules

```json
{
  "name": "protect-main",
  "target": "branch",
  "enforcement": "active",
  "bypass_actors": [
    { "actor_id": 0, "actor_type": "Integration", "bypass_mode": "always" }
  ],
  "conditions": { "ref_name": { "include": ["~DEFAULT_BRANCH"], "exclude": [] } },
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    { "type": "required_linear_history" },
    { "type": "pull_request", "parameters": {
        "required_approving_review_count": 0,
        "require_code_owner_review": false,
        "dismiss_stale_reviews_on_push": false,
        "require_last_push_approval": false,
        "required_review_thread_resolution": true,
        "allowed_merge_methods": ["squash"] } },
    { "type": "required_status_checks", "parameters": {
        "strict_required_status_checks_policy": true,
        "required_status_checks": [ { "context": "ci-ok" } ] } }
  ]
}
```

- `actor_id` for an `Integration` actor is the GitHub App's id. For the repository admin role
  (`actor_type: "RepositoryRole"`), the id is believed to be 5 [U]; confirm by reading a ruleset
  back after creating one.
- Tag ruleset: same endpoint with `"target": "tag"`, `ref_name.include: ["refs/tags/v*"]`, rules
  `deletion`, `non_fast_forward`, `update`, and `creation` with the release App as the only bypass.
- A status check context must match the workflow job name and must have run at least once before
  it can be selected in the UI.

## Other settings available through the API [V]

```bash
# Actions: read-only token, Actions may not approve pull requests
gh api -X PUT repos/OWNER/REPO/actions/permissions/workflow \
  -f default_workflow_permissions=read -F can_approve_pull_request_reviews=false
# Fork pull request workflows need approval
gh api -X PUT repos/OWNER/REPO/actions/permissions/fork-pr-contributor-approval \
  -f approval_policy=all_external_contributors
# Merge options, secret scanning and push protection
gh api -X PATCH repos/OWNER/REPO --input settings.json
# Private vulnerability reporting, Dependabot alerts and security updates
gh api -X PUT repos/OWNER/REPO/private-vulnerability-reporting
gh api -X PUT repos/OWNER/REPO/vulnerability-alerts
gh api -X PUT repos/OWNER/REPO/automated-security-fixes
```

`settings.json` carries `security_and_analysis.secret_scanning`, `secret_scanning_push_protection`,
`allow_squash_merge`, `allow_merge_commit`, `allow_rebase_merge`, `delete_branch_on_merge`,
`allow_auto_merge`, `squash_merge_commit_title` and `squash_merge_commit_message`. Requiring
SHA-pinned actions is a real setting (`sha_pinning_required`) [V].
https://docs.github.com/en/rest/actions/permissions

Web UI only, or not confirmed for the API [I]: CodeQL default setup, GitHub App creation and
installation, signing keys, npm and crates.io trusted-publisher configuration, the OpenSSF Best
Practices badge.

## Professional baseline

- **Actions hardening:** read-only default token; per-job `permissions`; every action pinned to a
  full commit SHA; never check out pull request code under `pull_request_target`; run `zizmor`
  (https://github.com/zizmorcore/zizmor) in CI [V for zizmor's scope in general; specific audit
  coverage not confirmed].
- **OpenSSF Scorecard** action with published results produces a badge; it checks branch
  protection, code review, dangerous workflows, pinned dependencies, token permissions, signed
  releases, security policy and update tooling [V]. https://github.com/ossf/scorecard . Expect a
  low code-review score for a solo repository.
- **Dependabot** covers cargo, npm, pip and github-actions natively [I].
- **Rust supply chain:** `cargo-deny` for licences, bans, advisories and sources [I].
- **Publishing:** npm trusted publishing via OIDC [V via search]
  (https://docs.npmjs.com/trusted-publishers); crates.io trusted publishing exists, with the first
  publish manual [V via search] (https://blog.rust-lang.org/2026/01/21/crates-io-development-update).
- **Licence:** comparable Rust tooling is MIT (oxc, Ruff) or MIT/Apache-2.0 dual (Biome, AccessKit)
  [V]. Dual MIT/Apache-2.0 is the Rust convention. Bundling MPL-2.0 crates applies file-level
  copyleft to those files only [I].
- **DCO versus CLA:** DCO is lighter; a CLA matters only if relicensing is planned [I].
- **Files:** CODEOWNERS (with workflows, `.github/` and release configuration explicitly owned),
  SECURITY.md pointing at private vulnerability reporting, CONTRIBUTING, CODE_OF_CONDUCT, issue and
  PR templates, GOVERNANCE.md stating who may merge and how that could be delegated [I].

## README elements worth copying [I]

From Rust tooling and accessibility projects (not fetched individually): a one-line pitch; a hero
example showing input and exact output; an honest status or accuracy statement with numbers;
install per runtime; a table of integrations; a "how this differs from X" section; badges for CI,
Scorecard and version. Guidepup's virtual screen reader README is the model for honesty: it says
the tool should not replace real screen reader testing and publishes its failing test counts [V].

## Not confirmed

Merge queue availability on personal repositories; the admin role actor id; whether provenance is
automatic under npm trusted publishing; the current names and state of Rust release tools
(release-plz, cargo-dist).
