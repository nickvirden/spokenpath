# VoiceOver through Guidepup

Whether Guidepup's macOS setup has been run on this Mac, and what one attempt to start VoiceOver
through Guidepup recorded. The answer is in
[ADR-0010](../../docs/decisions/ADR-0010-voiceover-capture-feasibility-and-corpus.md).

## Questions

- Has Guidepup's macOS setup been run (VoiceOver AppleScript control enabled)?
- Does Guidepup start VoiceOver on this Mac?

Whether two runs of the same key-press script record the same `spokenPhraseLog` is not answered
here. The script that drives VoiceOver through a page belongs to the VoiceOver capture job
(roadmap item 11), which writes it to the
[requirements below](#requirements-for-the-capture-script).

## Run

```sh
node spikes/voiceover-guidepup/check.mjs
```

[`check.mjs`](check.mjs) is read-only and needs no install. It checks whether
`/private/var/db/Accessibility/.VoiceOverAppleScriptEnabled` exists and runs
`defaults read com.apple.VoiceOver4/default SCREnableAppleScript`, and records whether VoiceOver is
running (`pgrep -x VoiceOver`). The setup counts as run only when the marker file exists and the
`defaults` value is `1`. It never runs `npx @guidepup/setup`, writes a setting, toggles a VoiceOver
preference, starts VoiceOver or uses `sudo`. The home directory, user name and host name are
replaced with `<redacted>` wherever a command echoes them.

| | Version |
|---|---|
| macOS | macOS 27.0.1 (26A434) |
| Safari | 27.0.1 |
| VoiceOver | 10 (bundle version 993) |
| Guidepup | 0.40.0, for the start attempt only; not a dependency of `spikes/` |
| Chromium | not used |
| Playwright | not used |
| Node | 24.13.0 |
| Date run | 2026-11-17 |

## Results

| Question | Result | Evidence |
|---|---|---|
| Guidepup's setup has been run | **pass**: the marker file exists and `SCREnableAppleScript` reads `1`; VoiceOver was not running | [`setup-check.json`](output/setup-check.json) |
| Guidepup starts VoiceOver | **fail**: `voiceOver.start()` throws "VoiceOver cannot be started", caused by "Failed to mount Guidepup preferences", which asks for `npx @guidepup/setup install`. The preferences image for macOS 27 (`guidepup-voiceover-preferences-xcode-27.dmg`) is listed in Guidepup's manifest but not installed | [`start-attempt.json`](output/start-attempt.json) |
| Speech captured, and two runs agree | **not run**: VoiceOver never started, so no phrase was captured and no second run was attempted | [`start-attempt.json`](output/start-attempt.json) |

### The start attempt

[`output/start-attempt.json`](output/start-attempt.json) is the raw record, kept byte for byte, of
one attempt on 2026-11-17 to start VoiceOver through Guidepup 0.40.0. The script that wrote it is
not committed. It was to open a three-element page (`three-elements.html`: a heading, a button and
a link) in Safari and move `next` four times, which is what the record's `page` and `steps` name;
it stopped at `voiceOver.start()`, so Safari was never opened. The record also holds, read-only and
as a boolean, whether the preferences image is present in Guidepup's cache
(`preferencesAsset.installed: false`).

Nothing was changed by the attempt. VoiceOver was not running afterwards
(`voiceOverRunningAfterStop` and `voiceOverRunningAtEnd` are false), no Guidepup volume was left
mounted, and a checksum and listing of VoiceOver's preference files taken before and after the
attempt match. No phrase was captured, so the record holds no speech.

### What `voiceOver.start()` does before VoiceOver runs

Read from the Guidepup 0.40.0 package source (`lib/macOS/VoiceOver/VoiceOver.js` and
`lib/macOS/VoiceOver/preferences/`): before launching VoiceOver, `start()` attaches the
preferences image from its cache with `hdiutil`, adds Guidepup's portable identifier to
`SCRCUserDefaultsAlwaysUsePortableIDs` in VoiceOver's `com.apple.VoiceOver4.local.plist`, creates
symlinks to the image's portable preferences in VoiceOver's preferences folder, and restarts
`cfprefsd`. On the attempt it stopped while locating the image, because the image is missing,
before changing anything. Once the image is installed, starting VoiceOver through Guidepup changes
VoiceOver's own preference files.

## Requirements for the capture script

The VoiceOver capture job writes the script that drives VoiceOver. Before its first run:

1. The maintainer has run Guidepup's setup on the capture Mac, including
   `npx @guidepup/setup install`, and accepts that Guidepup's start-up edits VoiceOver's preference
   files as described above. The setup is the maintainer's step; a job never runs it.
2. A way to restore VoiceOver's preferences after those edits (the portable identifier in
   `com.apple.VoiceOver4.local.plist`, the symlinks, the `cfprefsd` restart) is worked out and
   reviewed, because no run has reached them.

The script:

3. Refuses to start when a VoiceOver it did not start is running. It never runs Guidepup's setup,
   writes a setting or uses `sudo`, and it replaces the home directory, user name and host name in
   anything it records.
4. Has one overall deadline. It writes its output file as incomplete before anything else,
   including the version reads, so an interrupted run never leaves an earlier result looking
   current. When the deadline passes it writes the runs gathered so far, in the same shape as a
   normal finish and marked as past the deadline, then exits non-zero. A run that ends while the
   deadline path is writing cannot mark the result complete.
5. Tears VoiceOver down when the deadline passes during start-up. Guidepup 0.40.0's `stop()` throws
   "not running" until `start()` resolves, and Guidepup's own teardown (unmounting the image,
   restoring the user's VoiceOver) runs only on SIGINT, SIGTERM, SIGQUIT, SIGHUP and `beforeExit`,
   none of which `process.exit()` fires. The deadline path triggers that teardown, for example by
   sending its own process SIGTERM, and waits for a bounded time until `pgrep -x VoiceOver` is
   empty and `hdiutil info` lists no Guidepup volume, before it writes the result and exits. A
   `stop()` that never settles is raced against a short timer. A stub used to test this path
   throws from `stop()` until `start()` resolves, as Guidepup does.
6. Gives every child process a timeout (`osascript`, `defaults`, `sw_vers`). A synchronous call
   blocks the event loop, so a Safari that stops answering or an Automation permission prompt
   would otherwise hold the script past its deadline.
7. Opens the fixture in a new Safari window, polls until the front window's title is the
   fixture's (never a fixed sleep), reads that window's id only then, and closes only that window,
   both after each run and on the deadline path. A failed close is printed, not only recorded.
8. Serves the fixture from `127.0.0.1` without parsing the request target with `new URL()`: a
   target such as `//` makes it throw inside the request listener, which ends the process outside
   its cleanup and leaves VoiceOver running. It compares the path directly and answers 404 to
   every other request, and an `uncaughtException` handler runs the same cleanup as the deadline
   path.
9. Keeps only phrases spoken on the fixture. It drops every phrase before the fixture's heading,
   recording only how many, and cuts the log after the fixture's last element, because more
   `next` moves than elements can leave the page. The maintainer still reads the kept log before
   committing it and states in this README anything left out and how.
10. Treats a run in which the heading is never spoken as a failed run, and reports agreement only
    when both runs found the heading, so two empty logs never read as agreement.
11. Runs the same key presses in two separate VoiceOver sessions, compares the two
    `spokenPhraseLog` records phrase by phrase, and records the macOS version and build, the
    Safari version, the VoiceOver version and the Guidepup version with the capture.

Captured phrases are shown as illustrative speech, approximate: a record of one VoiceOver build on
one Mac, never a claim about anyone's experience of a screen reader.
