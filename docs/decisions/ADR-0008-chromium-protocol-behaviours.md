# ADR-0008: Chromium protocol behaviours the model must handle

## Status

Superseded by [ADR-0012](ADR-0012-chromium-protocol-behaviours-and-hidden-content-names.md)

## Date

2026-11-17

## Context

The Chromium adapter turns raw DevTools Protocol responses into the engine-neutral model
([ADR-0002](ADR-0002-browser-ground-truth-and-rust-core.md)): capture collects, the Rust core
interprets. Research left open how Chromium reports nodes it ignores, whether it names the
ancestor that hides a subtree, and several details of child order, text and name provenance
([architecture and Chromium capture](../research/architecture-and-chromium-capture.md), "Not
confirmed").

The [Chromium protocol probe](../../spikes/chromium-cdp/README.md) answers these on Chromium
159.0.8154.6 (Playwright 1.65.1, new headless mode) on macOS 27.0.1. It loads seven probe pages
over HTTP and records `DOM.getDocument({depth: -1, pierce: true})`, `Accessibility.getFullAXTree`,
and, for each probe element, `Accessibility.getPartialAXTree` by `backendNodeId` with and without
`fetchRelatives`.

| Question | Result | Raw output |
|---|---|---|
| (a) `getPartialAXTree` recovers role and name of ignored nodes (`aria-hidden`, `display:none`, `visibility:hidden`, `inert`, `role="presentation"`, `alt=""`) | fail: role `none`, no name, one reason per node | [`01-ignored.json`](../../spikes/chromium-cdp/output/01-ignored.json) |
| (a) a `<label for>` label is ignored | fail: it is a `LabelText` node, not ignored | [`01-ignored.json`](../../spikes/chromium-cdp/output/01-ignored.json) |
| (b) `ariaHiddenSubtree` identifies the hiding ancestor | pass: `relatedNodes[].backendDOMNodeId`, with `idref` when the ancestor has an `id` | [`02-subtree.json`](../../spikes/chromium-cdp/output/02-subtree.json) |
| (b) an `inert` subtree identifies the hiding ancestor | fail: the descendant reports `inertElement`, with no related node | [`02-subtree.json`](../../spikes/chromium-cdp/output/02-subtree.json) |
| (b) `notRendered` (`display:none` ancestor) identifies the hiding ancestor | fail: no related node | [`02-subtree.json`](../../spikes/chromium-cdp/output/02-subtree.json) |
| (b) `notVisible` (`visibility:hidden` ancestor) identifies the hiding ancestor | fail: no related node | [`02-subtree.json`](../../spikes/chromium-cdp/output/02-subtree.json) |
| (b) the DOM snapshot identifies `display:none` and `visibility:hidden` ancestors | pass: no layout node, and computed `visibility: hidden` | [`02-subtree.json`](../../spikes/chromium-cdp/output/02-subtree.json) |
| (c) AX child order follows `aria-owns` | pass | [`03-aria-owns.json`](../../spikes/chromium-cdp/output/03-aria-owns.json) |
| (c) AX child order follows shadow DOM slotting (named and default slots) | pass | [`04-shadow-slots.json`](../../spikes/chromium-cdp/output/04-shadow-slots.json) |
| (c) AX child order follows CSS `reading-flow` (flex and grid) | pass; without `reading-flow`, `order` and grid placement leave DOM order | [`05-reading-flow.json`](../../spikes/chromium-cdp/output/05-reading-flow.json) |
| (d) `StaticText` and `InlineTextBox` appear in `getFullAXTree` | pass: one `StaticText` per DOM text node, `InlineTextBox` per line fragment, with or without enabling the `Accessibility` domain | [`06-text-runs.json`](../../spikes/chromium-cdp/output/06-text-runs.json) |
| (e) `name.sources` for label-for | pass: `nativeSource: labelfor` | [`07-names.json`](../../spikes/chromium-cdp/output/07-names.json) |
| (e) `name.sources` for a wrapped label | pass: `nativeSource: labelwrapped` | [`07-names.json`](../../spikes/chromium-cdp/output/07-names.json) |
| (e) `name.sources` for `aria-labelledby` with `aria-label` | pass: `aria-labelledby` wins, `aria-label` is `superseded` | [`07-names.json`](../../spikes/chromium-cdp/output/07-names.json) |
| (e) `name.sources` for `title` only | pass: `attribute: title` | [`07-names.json`](../../spikes/chromium-cdp/output/07-names.json) |
| (e) `name.sources` for `placeholder` only | pass: `type: placeholder` | [`07-names.json`](../../spikes/chromium-cdp/output/07-names.json) |
| (e) `name.sources` for `img` with `alt` | pass: `attribute: alt` | [`07-names.json`](../../spikes/chromium-cdp/output/07-names.json) |
| (e) `name.sources` for `aria-labelledby` pointing at a hidden node | pass: the hidden text is used; the related node is an ignored `notRendered` node | [`07-names.json`](../../spikes/chromium-cdp/output/07-names.json) |

The probe README gives each answer with a pointer to the nodes that show it.

## Decision

Capture records what follows; every rule that reads meaning into it runs in the core.

1. **Ignored nodes.** Chromium gives no role or name for an ignored node, and a follow-up
   `getPartialAXTree` does not recover them. `getFullAXTree` leaves most ignored elements out
   altogether. Capture therefore calls `getPartialAXTree` (without `fetchRelatives`) for every
   DOM element that has no node in the full tree, and records the returned node and its
   `ignoredReasons` as they come. The model represents an ignored node by its DOM identity and its
   reasons, with role and name absent. It never fills them in from the tag or from another engine.
2. **Hiding ancestors.** For `ariaHiddenSubtree`, the hiding ancestor is
   `relatedNodes[].backendDOMNodeId`. For `inert`, `display:none` and `visibility:hidden` Chromium
   names no ancestor, and the AX parent chain skips the hiding elements. Capture therefore also
   records `DOMSnapshot.captureSnapshot` with the computed `display` and `visibility`, and the
   core finds the ancestor in the DOM: the nearest ancestor carrying `inert`; the topmost ancestor
   with no layout node whose parent has one; the topmost ancestor whose computed `visibility` is
   `hidden` under a visible parent.
3. **Child order.** The model takes children in AX `childIds` order, which already follows
   `aria-owns`, the flat tree of shadow slotting, and `reading-flow`. DOM order is used only to map
   nodes back to source. The model does not reorder children by CSS `order` or grid placement,
   because Chromium does not.
4. **Text runs.** Capture keeps `StaticText`, `LineBreak` and `InlineTextBox` nodes as delivered.
   The core builds one run of text per element by joining its `StaticText` descendants in AX
   order, with `LineBreak` contributing a line break. `InlineTextBox` nodes split a `StaticText`
   by visual line but carry no position and no DOM node, and one visual line can span several
   `StaticText` nodes, so the model does not build per-visual-line runs from the AX tree.
5. **Name provenance.** The source that produced a name is the one `name.sources` entry that has
   a `value` and no `superseded: true`. The model reads its `type`, `attribute`, `nativeSource` and
   `relatedNodes` as the provenance. Entries before it without a value were tried and empty;
   entries after it are superseded. A name taken from hidden content is detected by the core from
   the related node being ignored, since the source entry itself does not say so.
6. **Partial-tree responses are kept apart from the full tree.** A `getPartialAXTree` response can
   list a child twice in a parent's `childIds` and includes ancestors the full tree already holds.
   Capture stores those responses per queried node, and the model's tree comes from
   `getFullAXTree` alone.

## Consequences

- The snapshot schema needs, per DOM element, an optional AX node from the full tree, an optional
  partial-tree record for elements the full tree leaves out, and the DOM snapshot's layout
  membership and computed `display` and `visibility`.
- Excluded elements show their reason but no role or name. The output says the role and name are
  not exposed, rather than guessing them.
- Capture makes one extra protocol call per DOM element missing from the full tree, so its cost
  grows with the number of excluded elements; the probe did not measure it on large pages.
- Visual order that differs from DOM order without `reading-flow` is invisible to the model,
  matching what Chromium exposes.
- Node ids in the raw responses differ between runs, so capture goldens normalise them
  ([ADR-0007](ADR-0007-golden-test-strategy-and-drift-reports.md)).
- These behaviours belong to Chromium 159. A browser upgrade re-runs the probe, and a changed
  answer is reviewed as drift.

## Rejected alternatives

- **Recovering role and name of ignored nodes from `getPartialAXTree`.** The probe shows it
  returns role `none` and no name for every ignored case.
- **Deriving the role of an ignored element from its tag in capture.** That is interpretation in
  capture, which ADR-0002 forbids, and it would present a guessed role as one the browser exposed.
- **Finding hiding ancestors through the AX parent chain.** The chain skips the hiding elements
  for `inert`, `display:none` and `visibility:hidden`.
- **Reordering children by CSS `order` or grid placement.** Chromium keeps DOM order for those, so
  the model would report an order the browser does not expose.
- **Per-visual-line text from `InlineTextBox` nodes.** They carry no position, so line membership
  across sibling text nodes cannot be read from the tree.
