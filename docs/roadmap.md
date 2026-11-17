# Roadmap

spokenpath is pre-alpha. This page lists the planned work. The decisions behind it are in
[the decision records](decisions/README.md).

## Next, in order

Each item builds on the ones before it.

1. **Workspaces.** Cargo and pnpm workspaces, a pinned toolchain, lints and dependency checks, and
   a release step that moves every crate and package to the same version.
2. **Probes.** Small experiments that settle open questions before they are built on: what
   Chromium's DevTools Protocol returns for edge cases, and what Safari, its macOS accessibility
   tree and VoiceOver can be made to report. Each result is recorded in a decision record.
3. **Snapshot schema.** The versioned JSON format that capture writes and the core reads, for
   several engines from the start, with generated types and a validator.
4. **Source tagging.** `@spokenpath/instrument-html`, which tags each start tag with its source
   location without re-serialising the page
   ([ADR-0003](decisions/ADR-0003-source-tagging-by-splicing.md)).
5. **Chromium capture.** Raw DOM, the raw accessibility tree and a real Tab-key focus walk from
   Chromium, with the page served locally and external network blocked.
6. **Model.** The Rust engine-neutral model and the Chromium adapter: roles, names and their
   sources, descriptions, states, exclusions.
7. **Announcements.** Reading order, navigation indexes (headings, landmarks, links, form controls,
   tab order), Layer A and neutral Layer B
   ([ADR-0001](decisions/ADR-0001-two-layer-output.md)).
8. **CLI.** A command that takes an HTML file and prints the transcript as text or JSON.
9. **Safari cross-check.** Computed role and label from real Safari over WebDriver, with notes
   wherever Safari differs from Chromium.
10. **Safari tree backend.** A native macOS helper that reads Safari's accessibility tree, and a
    transcript per engine.
11. **VoiceOver corpus and speech profile.** VoiceOver speech captured locally on macOS and
    committed with its versions, and a "VoiceOver + Safari on macOS" profile that shows that
    phrasing only for captured patterns.
12. **Documentation.** An output reference, README examples produced by fixture tests, and the
    drift-report process ([ADR-0007](decisions/ADR-0007-golden-test-strategy-and-drift-reports.md)).

## Later, unordered

- iOS: Safari and VoiceOver on real devices.
- Other screen readers, such as NVDA and JAWS speech profiles.
- Flow diagrams of navigation paths.
- An HTML report.
- axe-core integration for rule checking, rather than a rule engine of spokenpath's own.
- Fix suggestions in tiers: safe, unsafe, and needs input (for content only a person can write,
  such as alt text and labels).
- React source mapping.
- A Rust accessible-name implementation, validated against web-platform-tests, for a no-browser
  mode.
- Language bindings (Node, WASM, Python).
- Publishing packages and crates.
