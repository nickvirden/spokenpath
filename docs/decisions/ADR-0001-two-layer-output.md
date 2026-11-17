# ADR-0001: Two-layer output: what the browser exposes, and illustrative speech

## Status

Accepted

## Date

2026-11-16

## Context

Standards define the accessibility tree, the accessible name and description computation, and the
mapping to platform accessibility APIs. None of them defines what a screen reader says: wording,
the order of role, name and state, verbosity and browse or focus mode are vendor-defined and vary
by screen reader, browser, operating system, version and user settings
([standards and evidence](../research/standards-and-evidence.md)). The W3C ARIA-AT project frames
expected behaviour as information conveyed, not as exact strings.

Every existing simulator hedges for this reason, and trust is the main risk to the project: one
public bad prediction can cost credibility with accessibility specialists
([market landscape](../research/market-landscape.md)).

## Decision

spokenpath's output has two layers, and every line says which layer it belongs to.

- **Layer A, "what the browser exposes".** Role, name, description, states, why a node is excluded
  from the accessibility tree, and where the name came from. Each Layer A is labelled with the
  browser engine and version it was captured from. It is exact for that browser build, including
  that browser's bugs, and is not claimed to conform to any specification.
- **Layer B, "illustrative speech, approximate".** A neutral illustrative phrase per node, always
  marked approximate, kept in a separate `approx` field in JSON output and removable with
  `--no-speech`. Phrasing attributed to a real screen reader appears only for patterns backed by
  captured speech, with the capture versions stated.
- Values the tool computes itself rather than reads from the browser (for example position in a
  set) are labelled "computed by tool".
- spokenpath makes no WCAG or other conformance claim.

## Consequences

- Every output format carries the engine and version, and every line carries its layer tag, so a
  reader can tell exposed data from approximation at a glance.
- Layer A inherits browser bugs. That is intended: it reports what the browser exposes, not what a
  specification says it should expose.
- Real screen reader phrasing needs a captured speech corpus first
  ([ADR-0006](ADR-0006-engine-scope.md)); until then Layer B is neutral wording only.
- User-facing text follows the wording rule in `AGENTS.md`: "what the browser exposes" for the
  data, "illustrative speech, approximate" for speech, no claim to reproduce what a screen reader
  says or what users experience, and no conformance claim.

## Rejected alternatives

- **A single "this is what VoiceOver says" output.** There is no specification of screen reader
  output to be faithful to, and speech varies by version and settings, so the claim cannot be
  backed and one wrong line discredits the whole tool.
- **Claiming Layer A conforms to the specifications.** Layer A is exact for one browser build,
  including that browser's bugs; where the browser deviates from the specification, a conformance
  claim would be false.
- **A WCAG conformance claim.** Showing what the browser exposes does not establish that a page
  meets WCAG, and conformance language invites comparison with overlay products that make such
  claims without grounds.
