# ADR-0010: VoiceOver capture feasibility and the speech corpus

## Status

Accepted

## Date

2026-11-17

## Context

[ADR-0006](ADR-0006-engine-scope.md) puts a "VoiceOver + Safari on macOS" speech profile in scope,
built from VoiceOver speech captured locally and committed as a corpus that CI replays and never
drives. [ADR-0001](ADR-0001-two-layer-output.md) shows that phrasing as illustrative speech,
approximate, and only for patterns the corpus covers. Guidepup can start VoiceOver and collect its
spoken phrases, but only after its macOS setup has enabled VoiceOver's AppleScript control, and
research left its repeat-run agreement unmeasured
([Safari and VoiceOver](../research/safari-and-voiceover.md), stage 3 of "Recommended staging" and
"Not verified").

The [VoiceOver probe](../../spikes/voiceover-guidepup/README.md) checks, read-only, whether that
setup has been run on the maintainer's Mac (macOS 27.0.1, Safari 27.0.1, VoiceOver 10), and would
run the speech probe only if it has. Running the setup is the maintainer's step, never a job's.

| Question | Result | Raw output |
|---|---|---|
| Guidepup's setup has been run on the capture Mac | fail: `/private/var/db/Accessibility/.VoiceOverAppleScriptEnabled` does not exist, and the `com.apple.VoiceOver4/default` defaults domain is not found | [`setup-check.json`](../../spikes/voiceover-guidepup/output/setup-check.json) |
| Two runs of the same key-press script give the same `spokenPhraseLog` | not run: setup pending. VoiceOver was not started and no Guidepup dependency was added | [`setup-check.json`](../../spikes/voiceover-guidepup/output/setup-check.json) |
| Speech capture on a hosted macOS runner | not run: ADR-0006 keeps VoiceOver out of CI, and the corpus does not depend on hosted runners | [workflow](../../.github/workflows/macos-probes.yml) |

The Safari side the corpus pairs with, including the rule for adding any macOS CI job, is in
[ADR-0009](ADR-0009-safari-capture-route-and-source-locations.md). The corpus depends on none of
the hosted-runner results recorded there, because speech is captured locally.

## Decision

1. **The corpus approach.** Speech is captured locally on a Mac, through Guidepup, with a fixed
   key-press script per fixture: the same keys in the same order on every run. Each capture is
   committed with the macOS version and build, the Safari version and the VoiceOver version. CI
   replays the committed corpus and never starts VoiceOver.
2. **Agreement is measured, not assumed.** Every fixture is captured at least twice in separate
   VoiceOver sessions, and the two `spokenPhraseLog` records are compared phrase by phrase. The
   corpus records the agreement for each fixture, and a pattern enters the speech profile only
   from fixtures whose runs agree.
3. **Status of the probe.** Speech capture is "not run: setup pending". It runs once the maintainer
   has run Guidepup's setup on the capture Mac, which
   [`check.mjs`](../../spikes/voiceover-guidepup/check.mjs) confirms read-only. The first job after
   that runs the probe exactly as its README states: a three-element page, `spokenPhraseLog`
   recorded twice, agreement reported, with the macOS, Safari and VoiceOver versions.
4. **Wording.** Phrasing from the corpus is labelled illustrative speech, approximate, and appears
   only for patterns the corpus covers; everything else stays as what the browser exposes. The
   corpus is a record of one VoiceOver build on one Mac. Nothing built on it claims to reproduce a
   screen reader or makes a conformance claim.

## Consequences

- The VoiceOver corpus and speech profile (roadmap item 11) cannot start until the maintainer runs
  Guidepup's setup on the capture Mac.
- The first speech capture also measures repeat-run agreement; if two runs disagree, the corpus
  approach needs another look before the profile is built, through a new ADR.
- Capture needs a Mac with VoiceOver's AppleScript control enabled, which is a deliberate local
  change by the maintainer and never part of the repository's tooling.
- A VoiceOver or macOS upgrade changes what is captured, so the corpus is re-captured and the
  difference reviewed as drift ([ADR-0007](ADR-0007-golden-test-strategy-and-drift-reports.md)).

## Rejected alternatives

- **Running Guidepup's setup from a job.** It changes system settings that only the maintainer
  decides on.
- **Capturing speech on hosted macOS runners.** ADR-0006 already rejects gating on it, and the
  corpus needs a Mac whose settings are known.
- **One capture per fixture.** It cannot show whether capture is repeatable, so the corpus would
  carry phrasing whose stability is unknown.
