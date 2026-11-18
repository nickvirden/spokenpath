# ADR-0012: Chromium protocol behaviours and the hidden-content rule for names

## Status

Accepted

## Date

2026-11-18

## Context

The Chromium adapter turns raw DevTools Protocol responses into the engine-neutral model
([ADR-0002](ADR-0002-browser-ground-truth-and-rust-core.md)). The
[Chromium protocol probe](../../spikes/chromium-cdp/README.md) records how Chromium 159.0.8154.6
reports ignored nodes, hiding ancestors, child order, text and name provenance; its results table
is in [ADR-0008](ADR-0008-chromium-protocol-behaviours.md), and its raw output is in
`spikes/chromium-cdp/output/`.

ADR-0008 detects a name taken from hidden content from the related node being ignored. The
committed captures show that rule is too wide. Chromium also ignores elements it does not hide:
a wrapping `<label>` is ignored with `labelFor`, so a native radio named by a visible wrapping
label (`fixtures/snapshots/chromium/set-position/radio-group.json`) would be reported as named
from hidden content, while the same label referenced through `for` is a `LabelText` node and is
not. `uninteresting`, `presentationalRole` and `emptyAlt` likewise leave content rendered and
part of the name. Only the reasons that remove content from what the user perceives, `aria-hidden`,
`inert`, `display: none` and `hidden` (`notRendered`), `visibility: hidden` (`notVisible`) and
content outside an active modal dialog, make a name one taken from hidden content.

## Decision

Capture records what follows; every rule that reads meaning into it runs in the core.

1. **Ignored nodes.** Chromium gives no role or name for an ignored node, and a follow-up
   `getPartialAXTree` does not recover them. Capture calls `getPartialAXTree` (without
   `fetchRelatives`) for every DOM element that has no node in the full tree, and records the
   returned node and its `ignoredReasons` as they come. The model represents an ignored node by
   its DOM identity and its reasons, with role and name absent. It never fills them in from the tag
   or from another engine.
2. **Hiding ancestors.** For `ariaHiddenSubtree`, the hiding ancestor is
   `relatedNodes[].backendDOMNodeId`. For `inert`, `display:none` and `visibility:hidden` Chromium
   names no ancestor, and the AX parent chain skips the hiding elements. Capture also records
   `DOMSnapshot.captureSnapshot` with the computed `display` and `visibility`, and the core finds
   the ancestor in the DOM: the nearest ancestor carrying `inert`; the topmost ancestor with no
   layout node whose parent has one; the topmost ancestor whose computed `visibility` is `hidden`
   under a visible parent.
3. **Child order.** The model takes children in AX `childIds` order, which follows `aria-owns`,
   the flat tree of shadow slotting, and `reading-flow`. DOM order is used only to map nodes back
   to source. The model does not reorder children by CSS `order` or grid placement, because
   Chromium does not.
4. **Text runs.** Capture keeps `StaticText`, `LineBreak` and `InlineTextBox` nodes as delivered.
   The core builds one run of text per element by joining its `StaticText` descendants in AX
   order, with `LineBreak` contributing a line break. The model does not build per-visual-line runs
   from `InlineTextBox` nodes.
5. **Name provenance.** The source that produced a name is the first `name.sources` entry that has
   a `value` and no `superseded: true`. The model reads its `type`, `attribute`, `nativeSource` and
   `relatedNodes` as the provenance. Entries before it without a value were tried and empty;
   entries after it are superseded. The core marks a name as taken from hidden content, computed by
   tool, when a related node of that source is an element the engine leaves out of the tree for a
   hiding reason: `aria-hidden`, `inert`, `notRendered`, `notVisible` or `activeModalDialog`. A
   related node ignored only for another reason, such as `labelFor`, `uninteresting`,
   `presentationalRole` or `emptyAlt`, does not make the name one from hidden content.
6. **Partial-tree responses are kept apart from the full tree.** A `getPartialAXTree` response can
   list a child twice in a parent's `childIds` and includes ancestors the full tree already holds.
   Capture stores those responses per queried node, and the model's tree comes from
   `getFullAXTree` alone.

This record supersedes [ADR-0008](ADR-0008-chromium-protocol-behaviours.md), whose probe results
remain the evidence for decisions 1 to 4 and 6.

## Consequences

- Excluded elements show their reason but no role or name; the output says the role and name are
  not exposed rather than guessing them.
- A name drawn from a visible label that Chromium folds into its control is not reported as
  hidden content, whichever way the label is associated.
- A reason name Chromium adds later counts as hiding only once the core lists it, so a new hiding
  reason is under-reported until then rather than a visible one being over-reported.
- Capture makes one extra protocol call per DOM element missing from the full tree.
- These behaviours belong to Chromium 159. A browser upgrade re-runs the probe, and a changed
  answer is reviewed as drift.

## Rejected alternatives

- **Any ignored related node means hidden content.** A wrapping label is ignored with `labelFor`
  while it stays visible and names the control, so the flag would call visible text hidden and the
  answer would depend on how the label is associated.
- **Reading a hidden-content flag from the engine.** Chromium's source entry does not say whether
  the text it used was hidden.
- **Recovering role and name of ignored nodes from `getPartialAXTree`.** The probe shows it
  returns role `none` and no name for every ignored case.
- **Deriving the role of an ignored element from its tag.** That presents a guessed role as one
  the browser exposed.
- **Finding hiding ancestors through the AX parent chain.** The chain skips the hiding elements
  for `inert`, `display:none` and `visibility:hidden`.
- **Reordering children by CSS `order` or grid placement.** Chromium keeps DOM order for those.
- **Per-visual-line text from `InlineTextBox` nodes.** They carry no position, so line membership
  across sibling text nodes cannot be read from the tree.
