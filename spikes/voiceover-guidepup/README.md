# VoiceOver through Guidepup

Whether VoiceOver speech can be captured on this Mac through Guidepup, and whether two runs of the
same key-press script agree. The answer is recorded in
[ADR-0010](../../docs/decisions/ADR-0010-voiceover-capture-feasibility-and-corpus.md).

## Questions

- Has Guidepup's macOS setup been run (VoiceOver AppleScript control enabled)?
- If it has: do two runs over a three-element page record the same `spokenPhraseLog`?

## Run

```sh
node spikes/voiceover-guidepup/check.mjs
```

[`check.mjs`](check.mjs) is read-only. It checks whether
`/private/var/db/Accessibility/.VoiceOverAppleScriptEnabled` exists and runs
`defaults read com.apple.VoiceOver4/default SCREnableAppleScript`, and records whether VoiceOver is
running (`pgrep -x VoiceOver`). It never runs `npx @guidepup/setup`, never writes either setting,
never toggles a VoiceOver preference and never starts VoiceOver. The home directory, user name and
host name are replaced with `<redacted>` wherever a command echoes them. The setup counts as run
only when the marker file exists and the `defaults` value is `1`.

| | Version |
|---|---|
| macOS | macOS 27.0.1 (26A434) |
| Safari | 27.0.1 |
| VoiceOver | 10 (bundle version 993) |
| Chromium | not used |
| Playwright | not used |
| Node | 24.13.0 |
| Date run | 2026-11-17 |

## Results

| Question | Result | Evidence |
|---|---|---|
| Guidepup's setup has been run | **fail**: the marker file does not exist (`ENOENT`) and `defaults` reports the domain `com.apple.VoiceOver4/default` not found | [`setup-check.json`](output/setup-check.json) |
| Two runs of `spokenPhraseLog` agree | **not run: setup pending**. No Guidepup dependency is added and VoiceOver is not started | [`setup-check.json`](output/setup-check.json) |

## What a later job must check

1. That the maintainer has run Guidepup's setup on the capture Mac: `check.mjs` reports
   `setupRun: true` (the marker file exists and `SCREnableAppleScript` reads `1`). The setup is the
   maintainer's step; a job never runs it.
2. Start VoiceOver through Guidepup, open a three-element page in Safari, and move through it with
   a fixed key-press script (the same keys, in the same order, every run).
3. Record `spokenPhraseLog` twice, in two separate VoiceOver sessions, and compare the two runs
   phrase by phrase; report how many phrases agree.
4. Record the macOS version and build, the Safari version and the VoiceOver version with the
   capture.

Captured phrases are shown as illustrative speech, approximate: a record of one VoiceOver build on
one Mac, never a claim about anyone's experience of a screen reader.
