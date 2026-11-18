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
setup has been run on the maintainer's Mac (macOS 27.0.1, Safari 27.0.1, VoiceOver 10), and keeps
the record of one attempt to start VoiceOver through Guidepup 0.40.0 before moving through a
three-element page in Safari. Running the setup is the maintainer's step, never a job's.

| Question | Result | Raw output |
|---|---|---|
| Guidepup's setup has been run on the capture Mac | pass: `/private/var/db/Accessibility/.VoiceOverAppleScriptEnabled` exists and `SCREnableAppleScript` reads `1` | [`setup-check.json`](../../spikes/voiceover-guidepup/output/setup-check.json) |
| Guidepup starts VoiceOver | fail: "VoiceOver cannot be started", caused by "Failed to mount Guidepup preferences"; Guidepup's VoiceOver preferences image for macOS 27 is not installed (`npx @guidepup/setup install` has not been run) | [`start-attempt.json`](../../spikes/voiceover-guidepup/output/start-attempt.json) |
| Two runs of the same key-press script give the same `spokenPhraseLog` | not run: VoiceOver did not start, so no phrase was captured and no second run was attempted | [`start-attempt.json`](../../spikes/voiceover-guidepup/output/start-attempt.json) |
| Speech capture on a hosted macOS runner | not run: ADR-0006 keeps VoiceOver out of CI, and the corpus does not depend on hosted runners | [workflow](../../.github/workflows/macos-probes.yml) |

Reading Guidepup 0.40.0's package source shows what its `start()` does before VoiceOver runs:
it attaches the preferences image, adds Guidepup's portable identifier to VoiceOver's local
preferences file, symlinks the image's portable preferences into VoiceOver's preferences folder,
and restarts `cfprefsd`. On the attempt it stopped while locating the image, before changing
anything: VoiceOver's preference files match before and after, and no VoiceOver process was left.

The Safari side the corpus pairs with, including the rule for adding any macOS CI job, is in
[ADR-0009](ADR-0009-safari-capture-route-and-source-locations.md). The corpus depends on none of
the hosted-runner results recorded there, because speech is captured locally.

## Decision

1. **The corpus approach.** Speech is captured locally on a Mac, through Guidepup, with a fixed
   key-press script per fixture: the same keys in the same order on every run. Each capture is
   committed with the macOS version and build, the Safari version, the VoiceOver version and the
   Guidepup version. CI replays the committed corpus and never starts VoiceOver.
2. **Agreement is measured, not assumed.** Every fixture is captured at least twice in separate
   VoiceOver sessions, and the two `spokenPhraseLog` records are compared phrase by phrase. The
   corpus records the agreement for each fixture, and a pattern enters the speech profile only
   from fixtures whose runs agree.
3. **Status of the probe.** Speech capture is "not run": the AppleScript part of Guidepup's setup is
   done, but Guidepup cannot start VoiceOver until its screen reader assets are installed
   (`npx @guidepup/setup install`). Installing them is the maintainer's step, and so is accepting
   that Guidepup's start-up edits VoiceOver's own preference files.
4. **The capture script belongs to the capture job.** The probe holds only the read-only setup
   check and the record of the start attempt. The script that drives VoiceOver through a page is
   written by the VoiceOver capture job (roadmap item 11) to the
   [requirements in the probe README](../../spikes/voiceover-guidepup/README.md#requirements-for-the-capture-script):
   tearing VoiceOver and Guidepup's preferences image down even when start-up stalls, a timeout on
   every call so the overall deadline holds, opening and closing only its own Safari window,
   serving the fixture without a request that can end the process, and keeping only phrases
   spoken on the fixture. Its first run is the first speech capture: a three-element page,
   `spokenPhraseLog` recorded in two separate VoiceOver sessions, agreement reported, with the
   macOS, Safari, VoiceOver and Guidepup versions.
5. **Wording.** Phrasing from the corpus is labelled illustrative speech, approximate, and appears
   only for patterns the corpus covers; everything else stays as what the browser exposes. The
   corpus is a record of one VoiceOver build on one Mac. Nothing built on it claims to reproduce a
   screen reader or makes a conformance claim.

## Consequences

- The VoiceOver corpus and speech profile (roadmap item 11) cannot start until the maintainer has
  installed Guidepup's screen reader assets on the capture Mac.
- Every capture through Guidepup 0.40.0 mounts Guidepup's preferences image and points VoiceOver
  at Guidepup's portable preferences, so the capture Mac's VoiceOver runs with Guidepup's settings
  during capture rather than the maintainer's own. Those settings are part of what the corpus
  records, and a Guidepup upgrade that changes them is reviewed as drift.
- The first speech capture also measures repeat-run agreement; if two runs disagree, the corpus
  approach needs another look before the profile is built, through a new ADR.
- Capture needs a Mac with VoiceOver's AppleScript control enabled and Guidepup's assets
  installed, both deliberate local changes by the maintainer and never part of the repository's
  tooling.
- A VoiceOver or macOS upgrade changes what is captured, so the corpus is re-captured and the
  difference reviewed as drift ([ADR-0007](ADR-0007-golden-test-strategy-and-drift-reports.md)).

## Rejected alternatives

- **Running Guidepup's setup from a job.** It changes system settings that only the maintainer
  decides on.
- **Capturing speech on hosted macOS runners.** ADR-0006 already rejects gating on it, and the
  corpus needs a Mac whose settings are known.
- **One capture per fixture.** It cannot show whether capture is repeatable, so the corpus would
  carry phrasing whose stability is unknown.
