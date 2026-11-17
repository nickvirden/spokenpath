# ADR-0003: Source tagging by splicing attributes into the original text

## Status

Accepted

## Date

2026-11-16

## Context

spokenpath traces each line of output back to the HTML tag that produced it. The browser does not
know source locations, so the location has to travel inside the DOM, as an attribute the capture
layer can read back.

Parsing a page and re-serialising it changes the DOM: entity normalisation, implied `tbody`,
adoption-agency clones, and `template` and `noscript` handling all differ from the original. parse5
with `sourceCodeLocationInfo` reports start-tag offsets (in UTF-16 code units) and gives no
location for implied elements
([architecture and Chromium capture](../research/architecture-and-chromium-capture.md)).

## Decision

- `@spokenpath/instrument-html` uses a parser only to locate start tags in the original text.
- A source attribute carrying the tag's location is spliced into the original text at each start
  tag. The document is never re-serialised.
- Instrumentation is proven, not assumed: the original and the instrumented page are both loaded,
  and their DOM trees must be equal apart from the added attribute.
- Source excerpts are taken from the original bytes by span, not from the browser's outer HTML.
- Elements with no source tag are classified by origin (for example implied or created by script)
  rather than left blank.

## Consequences

- Source locations are exact for elements written in the HTML file.
- Implied elements and elements created by script have no location, and the output says so.
- The schema has to name its offset units explicitly, because parse5 reports UTF-16 code units.
- Documents in encodings other than UTF-8, and scripts that enumerate attributes, are edge cases
  that need their own handling.

## Rejected alternatives

- **Parse and re-serialise the document with the attribute added.** Re-serialising changes the
  DOM, so the page captured would not be the page the developer wrote.
- **Taking source excerpts from the browser's outer HTML.** The browser's serialisation differs
  from the original text, so excerpts would not match the file.
- **Leaving untagged elements blank.** A blank location reads as a bug in the tool; classifying the
  element's origin tells the reader why there is no location.
