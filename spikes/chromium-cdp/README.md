# Chromium protocol probe

What Chromium's DevTools Protocol returns for the cases the Chromium adapter has to handle. The
answers are recorded in [ADR-0008](../../docs/decisions/ADR-0008-chromium-protocol-behaviours.md).

## Questions

- (a) Does a follow-up `Accessibility.getPartialAXTree` call recover role and name for nodes that
  are ignored or missing in `Accessibility.getFullAXTree`?
- (b) Do `ignoredReasons` identify the hiding ancestor for subtree reasons?
- (c) Which child order does the AX tree give under `aria-owns`, shadow DOM slotting and CSS
  `reading-flow`?
- (d) How do text runs appear (`StaticText`, `InlineTextBox`), and what must be merged?
- (e) What does `name.sources` contain for each common naming case?

## Run

```sh
npm ci --ignore-scripts --prefix spikes
spikes/node_modules/.bin/playwright install chromium webkit
node spikes/chromium-cdp/probe.mjs
```

[`probe.mjs`](probe.mjs) launches Playwright's bundled Chromium, opens a CDP session with
`context.newCDPSession(page)`, enables the `DOM` and `Accessibility` domains and loads each page in
[`pages/`](pages/) from the fictional origin `http://spokenpath.test/`, with every other request
aborted. For each page it records the raw `DOM.getDocument({depth: -1, pierce: true})` and
`Accessibility.getFullAXTree` responses, and for every element carrying `data-probe` the full-tree
nodes with that `backendDOMNodeId` plus `getPartialAXTree` by `backendNodeId` with and without
`fetchRelatives`. The text-runs page adds `Accessibility.getChildAXNodes` per `StaticText` node and
a full tree read in a separate context that never enabled the `Accessibility` domain; the subtree
page adds `DOMSnapshot.captureSnapshot` with the computed `display` and `visibility`.

| | Version |
|---|---|
| macOS | macOS 27.0.1 (26A434) |
| Safari | not used |
| Chromium | 159.0.8154.6 (`Chrome/159.0.8154.6`, revision `98a2592026af8d673b9fa83b626ac68bdf4ae78a`), as reported by `Browser.getVersion` |
| Mode | headless: the full Chromium build in its new headless mode (`channel: 'chromium'`), not the separate headless shell |
| Playwright | 1.65.1 |
| Node | 24.13.0 |
| Date run | 2026-11-17 |

Node ids (`backendDOMNodeId`, AX `nodeId`) are assigned per run and differ between runs; the
answers below point at nodes by their `data-probe` name, which is stable.

## Results

| Question | Result | Evidence |
|---|---|---|
| (a) `getPartialAXTree` recovers role and name of ignored nodes | **fail** | [`01-ignored.json`](output/01-ignored.json) |
| (b) `ignoredReasons` identify the hiding ancestor | **pass** for `aria-hidden`; **fail** for `inert`, `display:none` and `visibility:hidden` | [`02-subtree.json`](output/02-subtree.json) |
| (c) child order under `aria-owns` | **pass**: the AX tree follows `aria-owns` | [`03-aria-owns.json`](output/03-aria-owns.json) |
| (c) child order under shadow DOM slotting | **pass**: the AX tree follows the flat tree | [`04-shadow-slots.json`](output/04-shadow-slots.json) |
| (c) child order under CSS `reading-flow` | **pass**: the AX tree follows `reading-flow`; `order` and grid placement alone keep DOM order | [`05-reading-flow.json`](output/05-reading-flow.json) |
| (d) text runs | **pass**: `StaticText` per DOM text node, `InlineTextBox` per visual line fragment | [`06-text-runs.json`](output/06-text-runs.json) |
| (e) `name.sources` for seven naming cases | **pass**: the winning source is identifiable in every case | [`07-names.json`](output/07-names.json) |

### (a) Ignored nodes

`getFullAXTree` leaves most ignored elements out entirely: `display-none`, `visibility-hidden`,
`inert`, `presentation` and `alt-empty` have no node in `fullAXTree.nodes`. Only `aria-hidden` is
present, as an ignored node. A follow-up `getPartialAXTree` returns a node for every one of them,
with and without `fetchRelatives`, but always with `role` `{"type": "role", "value": "none"}`,
`chromeRole` `0`, no `name`, and one reason: `ariaHiddenElement`, `notRendered`, `notVisible`,
`inertElement`, `presentationalRole` or `emptyAlt` respectively (see `targets[].partialWithoutRelatives`
for each `probe`). The role and name the element would have had are not recovered.

The `<label for>` element (`label-for`) is not ignored: it is a `LabelText` node with an empty name
and a `StaticText` child carrying the text.

### (b) Hiding ancestors

- `aria-hidden-subtree` and `aria-hidden-subtree-no-id` are ignored with `ariaHiddenSubtree`, whose
  value is `{"type": "idref", "relatedNodes": [{"backendDOMNodeId": …, "idref": "hider-aria"}]}`.
  The `backendDOMNodeId` is the hiding `div`; `idref` is present only when the ancestor has an
  `id`.
- `inert-subtree` reports `inertElement`, the same reason as an element that is itself `inert`;
  no `inertSubtree` reason and no `relatedNodes` appear.
- `not-rendered-subtree` reports `notRendered` and `visibility-subtree` reports `notVisible`, with
  no field naming the ancestor.
- With `fetchRelatives`, the ancestors returned for these three skip the hiding `div` and `p`: the
  node's `parentId` is the `main` node. The AX tree never contains the hiding ancestor.
- The ancestor can be found from the DOM instead: `inert` is an attribute in `domDocument`, and in
  `domSnapshot` the `hider-display` subtree has no layout node while its parent has one, and
  `hider-visibility` is the topmost node whose computed `visibility` is `hidden`.

### (c) Child order

- `aria-owns`: the `owner` list's AX children are `dom-first`, `dom-second`, then `owned`; the
  `original-parent` node keeps only the paragraph.
- Slotting: the `host` group's AX children are the shadow heading, the `title` slot (holding
  `light-titled`), the shadow paragraph, then the default slot (holding `light-default-a` and
  `light-default-b`). The slots appear as ignored `uninteresting` nodes. DOM order of the light
  children is A, titled, B.
- `reading-flow`: `CSS.supports` reports both `flex-visual` and `grid-rows` as supported
  (`cssSupports`). Under `flex-reading-flow` and `grid-reading-flow` the AX children are in visual
  order (DOM 2, DOM 3, DOM 1); under `flex-order-only` and `grid-placement-only` they stay in DOM
  order (DOM 1, DOM 2, DOM 3) although the buttons are displayed in another order.

### (d) Text runs

The `mixed` paragraph's AX children are `StaticText` "Plain start ", `strong` (with `StaticText`
"strong part"), `StaticText` " then a ", `link` (with `StaticText` "link"), `LineBreak` "\n" and
`StaticText` "second line after a break.". Each `StaticText` has `InlineTextBox` children; the
`wrapped` paragraph's single `StaticText` has four, one per visual line. `InlineTextBox` nodes have
negative ids, no `backendDOMNodeId` and no position data. They appear in `getFullAXTree` whether or
not the `Accessibility` domain was enabled (`fullAXTreeWithoutAccessibilityEnable` has the same 11
`InlineTextBox` nodes), and `getChildAXNodes` returns the same children (`staticTextChildren`).

### (e) Name sources

Each target's `fullTreeNodes[0].name.sources` lists every source Chromium considered, in order. The
source that produced the name is the one entry that has a `value` and no `superseded: true`:

| `data-probe` | Name | Winning source |
|---|---|---|
| `label-for` | "Label for text" | `type: relatedElement`, `nativeSource: labelfor`, `relatedNodes` the label |
| `wrapped-label` | "Wrapped label text" | `type: relatedElement`, `nativeSource: labelwrapped` |
| `labelledby-over-label` | "Labelledby text" | `attribute: aria-labelledby`, with `relatedNodes`; `aria-label` and contents follow as `superseded` with their values |
| `title-only` | "Title text" | `attribute: title` (a second, `superseded` `title` entry follows) |
| `placeholder-only` | "Placeholder text" | `type: placeholder`, `attribute: placeholder` |
| `img-alt` | "Alt text" | `attribute: alt` |
| `labelledby-hidden` | "Hidden label text" | `attribute: aria-labelledby`; the hidden target's text is used and nothing marks it as hidden |
