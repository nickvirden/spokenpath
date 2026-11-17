# VoiceOver through Guidepup

Whether VoiceOver speech can be captured on this Mac through Guidepup, and whether two runs of the
same key-press script agree. The answer is recorded in
[ADR-0010](../../docs/decisions/ADR-0010-voiceover-capture-feasibility-and-corpus.md).

## Questions

- Has Guidepup's macOS setup been run (VoiceOver AppleScript control enabled)?
- If it has: does Guidepup start VoiceOver, and do two runs over a three-element page record the
  same `spokenPhraseLog`?

## Run

```sh
npm ci --ignore-scripts --prefix spikes
node spikes/voiceover-guidepup/check.mjs
node spikes/voiceover-guidepup/probe.mjs; echo exit=$?; pgrep -x VoiceOver || echo voiceover-not-running
```

[`check.mjs`](check.mjs) is read-only. It checks whether
`/private/var/db/Accessibility/.VoiceOverAppleScriptEnabled` exists and runs
`defaults read com.apple.VoiceOver4/default SCREnableAppleScript`, and records whether VoiceOver is
running (`pgrep -x VoiceOver`). The setup counts as run only when the marker file exists and the
`defaults` value is `1`.

[`probe.mjs`](probe.mjs) uses `@guidepup/guidepup` 0.40.0, pinned in
[`spikes/package.json`](../package.json) and installed with `--ignore-scripts`. It refuses to start
when a VoiceOver it did not start is running. For each of two runs it calls `voiceOver.start()`,
opens [`pages/three-elements.html`](pages/three-elements.html) (a heading, a button and a link,
served from `127.0.0.1`) in a new Safari window, polls until that window is in front (recording
its title as `frontWindowTitle`), interacts with the web area, clears the phrase log, moves
`next` four times, and records `spokenPhraseLog()`. It keeps only the phrases from the first one
that speaks the fixture heading ("Probe heading") onwards, and records how many earlier phrases it
dropped (`droppedBeforeHeading`), never their text. The window it opened is closed and VoiceOver
is stopped in a `finally` after every run, and the probe records whether a VoiceOver process is
left afterwards. A run that errors is recorded with its error text and is not retried.

The whole probe has a three-minute deadline. The output file is rewritten with
`completed: false` before anything else runs, so an interrupted run never leaves an earlier result
looking current. When the deadline passes, the probe closes its window, stops VoiceOver, writes
the runs gathered so far with `deadlineExceeded: true`, and exits non-zero. Both paths write the
file through one function, so it has the same shape either way. That deadline path has been
exercised only against a stub Guidepup whose `start()` never resolves.

Everything after a successful `voiceOver.start()` (the window, the polling, the phrase filter and
the comparison) has never run on a real machine, because start-up has failed on every run so far.
A first real run may still need fixes there. It
also records, read-only and as a boolean, whether the VoiceOver preferences image that Guidepup
starts VoiceOver with is present in Guidepup's cache.

Neither script runs `npx @guidepup/setup`, writes a setting, toggles a VoiceOver preference or
uses `sudo`. The home directory, user name and host name are replaced with `<redacted>` wherever
a command or error echoes them.

| | Version |
|---|---|
| macOS | macOS 27.0.1 (26A434) |
| Safari | 27.0.1 |
| VoiceOver | 10 (bundle version 993) |
| Guidepup | 0.40.0 |
| Chromium | not used |
| Playwright | not used |
| Node | 24.13.0 |
| Date run | 2026-11-17 |

## Results

| Question | Result | Evidence |
|---|---|---|
| Guidepup's setup has been run | **pass**: the marker file exists and `SCREnableAppleScript` reads `1`; VoiceOver was not running | [`setup-check.json`](output/setup-check.json) |
| Guidepup starts VoiceOver | **fail**: `voiceOver.start()` throws "VoiceOver cannot be started", caused by "Failed to mount Guidepup preferences", which asks for `npx @guidepup/setup install`. The preferences image for macOS 27 (`guidepup-voiceover-preferences-xcode-27.dmg`) is listed in Guidepup's manifest but not installed | [`speech-runs.json`](output/speech-runs.json) |
| Speech captured, and two runs agree | **not run**: VoiceOver never started, so no phrase was captured and the second run was not attempted | [`speech-runs.json`](output/speech-runs.json) |

VoiceOver was not running after the run, no Guidepup volume was left mounted, and a checksum and
listing of VoiceOver's preference files taken before and after the run match. Since no phrase was
captured, the committed output holds no speech and needed no filtering.

### What `voiceOver.start()` does before VoiceOver runs

Read from the installed package (`lib/macOS/VoiceOver/VoiceOver.js` and
`lib/macOS/VoiceOver/preferences/`): before launching VoiceOver, Guidepup 0.40.0 attaches the
preferences image from its cache with `hdiutil`, adds Guidepup's portable identifier to
`SCRCUserDefaultsAlwaysUsePortableIDs` in VoiceOver's `com.apple.VoiceOver4.local.plist`, creates
symlinks to the image's portable preferences in VoiceOver's preferences folder, and restarts
`cfprefsd`. On this run it stopped while locating the image, because the image is missing, before
changing anything. Once the image is installed, starting VoiceOver through Guidepup changes
VoiceOver's own preference files.

## What a later job must check

1. That the maintainer has run Guidepup's setup on the capture Mac, including
   `npx @guidepup/setup install`, so `speech-runs.json` records `preferencesAsset.installed: true`.
   The setup is the maintainer's step; a job never runs it. The maintainer also accepts that
   Guidepup's start-up edits VoiceOver's preference files as described above.
2. Run `probe.mjs`: VoiceOver starts through Guidepup, Safari shows the three-element page, and the
   same four `next` moves run in each of two separate VoiceOver sessions.
3. Compare the two `spokenPhraseLog` records phrase by phrase (`agreement` in the output).
4. Check `frontWindowTitle` is "Three elements" and `headingFound` is true for both runs. The
   probe has already dropped every phrase spoken before the fixture heading
   (`droppedBeforeHeading` counts them). Still read the kept log before committing it, and leave
   out anything not from the fixture, saying in this README what was left out and how.
5. Record the macOS version and build, the Safari version, the VoiceOver version and the Guidepup
   version with the capture.

Captured phrases are shown as illustrative speech, approximate: a record of one VoiceOver build on
one Mac, never a claim about anyone's experience of a screen reader.
