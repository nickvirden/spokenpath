# ADR-0013: The command-line tool and the transcript output

## Status

Accepted

## Date

2026-11-19

## Context

Instrumentation (`@spokenpath/instrument-html`), Chromium capture (`@spokenpath/capture-chromium`),
the engine-neutral model (`spokenpath_model`) and the transcript (`spokenpath_announce`) each work
on their own, but nothing connects them, and nothing prints a transcript a person can read. A
developer or reviewer needs one command that takes an HTML file and prints what the browser
exposes, item by item, traced to the tags in that file, and a JSON form of the same for tools.

Capture is TypeScript because it drives the browser through Playwright; interpretation is Rust
([ADR-0002](ADR-0002-browser-ground-truth-and-rust-core.md)). The output follows the two-layer
rule of [ADR-0001](ADR-0001-two-layer-output.md). A snapshot is untrusted input: its strings are
page-controlled and reach a terminal, its source paths name files, and adapting it costs more
than reading it, not linearly in every shape. The sign-in fixture's capture is about 45 KB and
the largest committed capture (`focus/long.json`) is about 1 MB.

## Decision

1. **A Node orchestrator and a Rust child process.** `spokenpath <page.html>` (`packages/cli`,
   run as `pnpm spokenpath`) instruments and captures the page with `captureChromium`, then runs
   the `spokenpath-core` binary (`crates/spokenpath_cli`) as a child process, with the binary's
   stdout and stderr inherited unchanged. It resolves the binary as `target/debug/spokenpath-core`
   under the repository root, or under `$CARGO_TARGET_DIR/debug` when that is set.
2. **The snapshot on stdin.** The CLI writes the snapshot JSON to the binary's stdin
   (`spokenpath-core transcript -`), so no temporary file is written or cleaned up.
3. **Source bytes only from the named file, matched by SHA-256.** The binary never opens a path
   written inside a snapshot. Source bytes come only from the file named by `--source`; the binary
   hashes it and uses it for the snapshot source with the same `sha256`, and a file that matches no
   source is an error. Without `--source` the text output has locations and no excerpts. Excerpts
   are the start tag's bytes from that file, never the browser's serialisation.
4. **Exit codes and a size limit.** The binary exits 0 on success, 2 for a usage error or bad input
   (an unreadable file, an oversized input, a reader or adapter error, an unmatched `--source`), 3
   for an unsupported snapshot major version, and 1 only when writing stdout fails. Every failure
   writes one `spokenpath-core: ` line to stderr with the reader's or adapter's own message. The
   snapshot and the `--source` file are each limited to 32 MiB, checked before parsing. The CLI
   exits 2 for a usage error, an unreadable page or a page the instrumenter refuses, 1 for any
   other capture failure or a missing binary, and otherwise the binary's code. Capture diagnostics
   go to stderr in both formats and change neither the exit code nor stdout.
5. **JSON is `Transcript` serialised, with a generated schema.** `--format json` writes the announce
   crate's `Transcript` as `serde_json` serialises it, with nothing added, renamed, removed or
   filtered, and raw strings. Its JSON Schema, `schema/transcript.v0.json`, is generated from the
   Rust types by `cargo xtask schema` with the snapshot schema's settings and checked in CI.
6. **The text layout.** A header (spokenpath version, snapshot version, engine and browser
   version, viewport) and a legend; then every item in reading order under its JSON number,
   indented by depth, with its `A` line, source location and excerpt, name provenance, description
   and `~` line; then the excluded elements, the four navigation indexes and the tab order. Nothing
   is filtered: the root, unnamed generic containers, text runs that repeat their parent's name and
   list markers are items, an empty phrase prints `~ ""`, and every unnamed item says
   `(no accessible name)`. The text leaves out the role origin and the engine's other properties,
   which the JSON keeps. Items deeper than 32 levels are indented no further and state their depth.
   The golden of the sign-in page's text output, `fixtures/transcripts/sign-in.txt`, changes only
   with a pinned-browser upgrade or a printer change, and is reviewed as drift
   ([ADR-0007](ADR-0007-golden-test-strategy-and-drift-reports.md)). `docs/usage.md` is the key to
   every line.
7. **The terminal escaping set.** Every page-controlled string the text output or a stderr message
   prints passes through `spokenpath_announce::escape_for_terminal`, and the CLI's own messages
   through the TypeScript `escapeForTerminal`, which escapes the same set: control characters,
   bidirectional controls and marks, the line and paragraph separators, zero-width characters,
   the invisible operators U+2061 to U+2064, and the Unicode tag characters U+E0000 to U+E007F,
   each as `\u{..}`. Variation selectors pass unchanged.

## Consequences

- One command prints the transcript, and the same binary turns any committed snapshot into text
  or JSON without a browser, which is what the text goldens and the schema tests use.
- The command runs from a checkout and needs the Rust toolchain to build the binary; there is no
  install and no release binary yet.
- Under `pnpm spokenpath`, relative page paths resolve against the repository root, and pnpm
  prints its own script banner on stdout unless it runs with `-s`.
- A `--source` that differs from the captured page by one byte is refused, so a stale or wrong file
  never prints wrong excerpts silently.
- The JSON output's shape is the `Transcript` type's; a change to the types is a change to the
  schema, which `cargo xtask schema --check` makes visible. It carries no snapshot version,
  viewport or diagnostics.
- Escaping tag characters makes subdivision flag emoji show as escapes, and page text that
  literally contains `\u{..}` looks the same as an escaped character; the JSON keeps the raw
  strings.
- A new `e2e` CI job installs the pinned Chromium and runs the whole pipeline, and `ci-ok` needs it.

## Rejected alternatives

- **Native Node bindings or WASM for the core.** They put a build of the Rust core into the Node
  package and its platform matrix, and tie the core's release to Node's ABI or a WASM toolchain.
  A child process keeps the boundary the snapshot already is, and the binary is usable alone.
- **A temporary snapshot file.** It needs a writable location and cleanup on every exit path, and
  leaves page data on disk when cleanup fails; stdin needs neither, and the size limit applies the
  same way.
- **Reading the source from the snapshot's own path.** It lets a snapshot name any file for the
  binary to read and print. Matching `--source` by length only misses same-length edits.
- **No excerpts from the binary.** The transcript would lose the tag text that makes a location
  readable at a glance.
- **Hiding unnamed generic containers and duplicate text runs in the text.** The indexes and the tab
  order cite item numbers, and a hidden item would break those references; the full list is also
  the honest record.
- **Diagnostics in a text-only section.** The JSON shape is fixed as `Transcript`, so the two
  formats would report them differently; stderr serves both and leaves stdout and the goldens
  unchanged.
- **Exit 1 for a refused or unreadable page, as the capture CLI uses.** It gives 2 two meanings
  across the command; 2 means bad input throughout.
- **Passing variation selectors and tag characters alike.** Tag characters are invisible in a
  terminal and a known route for hidden text; variation selectors are needed for emoji.
