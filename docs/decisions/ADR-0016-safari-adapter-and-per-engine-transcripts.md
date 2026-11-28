# ADR-0016: The Safari adapter and per-engine transcripts

## Status

Accepted

## Date

2026-11-27

## Context

Most desktop VoiceOver users pair it with Safari ([ADR-0006](ADR-0006-engine-scope.md)), and
WebKit builds its own accessibility tree with its own role vocabulary. Capture already records
Safari's macOS accessibility tree raw as a `safari-ax` snapshot
([ADR-0015](ADR-0015-safari-ax-tree-capture.md)), every node with a source span when its element
carries a class token ([ADR-0009](ADR-0009-safari-capture-route-and-source-locations.md)). The
Rust core interprets, capture never does
([ADR-0002](ADR-0002-browser-ground-truth-and-rust-core.md)), and output keeps Layer A, what the
browser exposes, apart from Layer B, illustrative speech
([ADR-0001](ADR-0001-two-layer-output.md)).

The 46 committed `safari-ax` captures (Safari 27.0.1, macOS 27.0.1) ground the rules below. Names
are in `AXTitle` (contents, labels, `aria-labelledby`) or `AXDescription` (`aria-label`, `alt`, a
`title` used as the name), equal wherever both are set; a `title` used as a description is in
`AXHelp`; `aria-describedby` text is in no readable string attribute. Safari exposes no exclusion
reasons, no name provenance and no focus walk. Most roles are `AXGroup`, `AXStaticText` and the
controls; `AXSubrole` is read error `-25212` (no value) on most nodes. Safari has no node for
`optgroup` and its options, layout tables, the sign-in `svg`, `legend` or `body`. 13 non-text nodes
carry no token.

## Decision

1. **The model can say a part is not supplied.** The excluded nodes, the focus walk and each
   name's sources are each an `Availability`: `Available` with what the engine supplied, possibly
   empty, or `NotAvailable`. The Chromium adapter fills all three as available, exactly as before.
   `Role` gains `Platform { role, subrole }`, which is not an ARIA token, and the model carries the
   Safari build and the macOS version and build as optional fields.
2. **Integrity.** `spokenpath_model::safari_ax::adapt` runs the Chromium adapter's span checks,
   moved to a module both adapters share, before it reads any node. It rejects, naming the dotted
   path: a payload with no nodes; node 0 with a `parent`; any other node without one; a node listed
   in a `children` list other than its `parent`'s, or listed twice; node 0 listed as a child; and
   nodes their parents do not reach. Every walk is iterative. It does no file, network or
   environment I/O.
3. **Tree and text runs.** The tree is node 0 and its descendants through `children`, in Safari's
   `AXChildren` order; nothing is reordered and `AXChildrenInNavigationOrder` is not read. An
   `AXStaticText` with no children, no span and a string `AXValue` is a text run of that value;
   adjacent runs are not merged. A node with a span is located by it; without one, an `AXWebArea`
   is `document`, an `AXStaticText` is `text`, and anything else is `unknown`.
4. **Roles.** These pairs map to an ARIA token, each with no subrole unless one is named:
   `AXHeading` `heading`, `AXLink` `link`, `AXButton` `button`, `AXCheckBox` `checkbox`,
   `AXRadioButton` `radio`, `AXRadioGroup` `radiogroup`, `AXTextField` and `AXTextArea` `textbox`,
   `AXImage` `image`, `AXList`/`AXContentList` `list`, and `AXGroup` with `AXLandmarkBanner`,
   `AXLandmarkComplementary`, `AXLandmarkContentInfo`, `AXLandmarkForm`, `AXLandmarkMain`,
   `AXLandmarkNavigation`, `AXLandmarkRegion` or `AXLandmarkSearch` as `banner`, `complementary`,
   `contentinfo`, `form`, `main`, `navigation`, `region` or `search`. An absent `AXSubrole` or
   error `-25212` is no subrole; any other non-string `AXSubrole` makes the pair unmappable. Every
   other role stays the platform's text, because each stands for more than one ARIA role in WebKit:
   `AXGroup` with no subrole is `p`, `div`, `li`, a wrapping `label` or a `caption`; plain `AXList`
   is the `listbox`; `AXPopUpButton` is a `select` and a popup button; `AXStaticText` is text, a
   `label for` element and a `role=option` item; `AXTable`, `AXRow`, `AXCell` cover data and grid
   tables; `AXMenuItem` and `AXIncrementor` cover several widgets. The table holds only pairs a
   committed capture shows; adding one changes Safari goldens only.
5. **Name, description, value, states, properties.** The name is a non-empty string `AXTitle`,
   else a non-empty string `AXDescription`; its sources are not available. A non-empty `AXHelp` is
   the description, with no provenance. A non-empty string `AXValue` is the value, except on a
   text run and where it fills a state. `AXEnabled` fills `disabled` negated; `AXFocused`,
   `AXSelected`, `AXExpanded` and `AXRequired` fill their states; a string `AXInvalid` fills
   `invalid`. An `AXValue` of 1 to 9 on `AXHeading` fills `level`, of 0, 1 or 2 on `AXCheckBox`
   fills `checked` (false, true, mixed), of 0 or 1 on `AXRadioButton` fills `checked`, integer or
   whole-valued float alike; any other value fills nothing. Every other string, boolean or number
   attribute is a property in name order, `AXRole` and `AXSubrole` always among them; the
   display-dependent attributes (`AXPosition`, `AXFrame`, `AXSize`, `_AXPrimaryScreenHeight`,
   `AXVisibleCharacterRange`, `AXLayoutCount`) and every other value type are left out. Set
   positions are not computed; the snapshot's diagnostics are the model's.
6. **Not available from this engine.** A transcript lists the parts its engine does not supply in
   `not_available` (`name-sources`, `excluded`, `tab-order`) and leaves out `excluded`,
   `tab_order` and every name's `sources`; the list is omitted when empty, so Chromium JSON is
   unchanged. The text prints `name sources not available from this engine` (or
   `no accessible name; name sources not available from this engine`), and `not available from
   this engine` as the body of Excluded and Tab order, with no ending line.
7. **Platform roles in output.** A platform role prints as `AXGroup (platform role)` or
   `AXGroup/AXFieldset (platform role)`, with JSON `kind: "platform"` and a separate `subrole`. It
   is unspoken in Layer B, like an engine-internal role, and qualifies for no index; no template,
   phrase or state word changes. The Safari header is
   `Safari <version> on macOS <version> (<build>)` in the existing engine slot.
8. **The aligned layout.** `spokenpath-core transcript <chromium> --align-with <safari-ax>` sets
   Safari's transcript beside Chromium's. Two items align when both are accessible items located by
   a span with the same span id. Rows follow Chromium's reading order: a Chromium item with a span
   starts, at the first Chromium item carrying it, a row holding every item of both engines with
   that span; any other Chromium item is a row of its own. Safari items in no aligned row are
   grouped the same way among themselves and placed right after the rows of the nearest earlier
   aligned Safari item and the Chromium-only rows that follow them, in Safari order; those before
   the first aligned Safari item go right before the first aligned row. The text prints the
   Chromium header, an `aligned with` header for Safari, a legend, and one block per row: its
   `C<n>`/`S<n>` labels and location, then one `A Chromium` or `A Safari` line per item, and an
   `only in Chromium` or `only in Safari` line for a span one engine alone carries. Nothing else
   follows; the per-engine outputs keep names' provenance, `~` lines, indexes and the rest. JSON is
   the Chromium transcript with `aligned: { safari, rows }`.
9. **The options.** `spokenpath --engine chromium|safari|both`: `chromium` is the default;
   `safari` captures with `captureSafariAx` and prints its transcript; `both` captures in Chromium,
   then Safari, and runs the binary with the Safari snapshot on fd 3 as `--align-with /dev/fd/3`.
   `--cross-check safari` goes with `--engine chromium` only, and `--verify-instrumentation` with
   neither `safari` nor `both`. The binary picks the adapter by the snapshot's payload; a
   `safari-webdriver` snapshot, `--cross-check` or `--align-with` with a snapshot that is not
   `chromium`, an aligned snapshot that is not `safari-ax`, and an aligned snapshot from another
   page each exit 2 with one line. `--cross-check` and `--align-with` together are a usage error.
10. **Exit codes.** This amends [ADR-0013](ADR-0013-command-line-tool-and-transcript-output.md)
    Decision 4 and [ADR-0014](ADR-0014-safari-webdriver-cross-check.md) Decision 10:
    `unsupported-platform` exits 2 for `--cross-check safari`, `--engine safari` and
    `--engine both`, checked from the command line before anything runs; every other Safari capture
    failure exits 1 (SIGINT and SIGTERM 130 and 143), with no transcript. The mapping lives in
    `packages/cli/src/exit.ts`.

## Consequences

- A reviewer can read Safari's macOS accessibility tree item by item and see, per source tag, how
  the two engines differ, without either engine's gaps filled from the other.
- Safari items carry no name provenance, no exclusion list and no tab order, and say so; a Safari
  set position is absent.
- Untagged Safari nodes and text runs never align, so a page with much untagged structure shows
  many one-engine rows.
- Safari goldens change when a role pair joins the table, and `--engine safari|both` runs only on a
  Mac with the Accessibility permission; CI checks the committed captures on every system.
- `--cross-check safari` off macOS now exits 2, which a script that matched 1 must follow.

## Rejected alternatives

- **Speaking platform roles,** as their text or as `AXRoleDescription`: Layer B speaks only ARIA
  tokens, and a role description reads like VoiceOver's own phrasing, which ADR-0001 allows only
  from a captured corpus.
- **Mapping every role WebKit can report,** such as `AXPopUpButton` to `combobox`: each listed
  exception stands for more than one ARIA role, so a token would claim more than Safari exposes.
- **Filling Safari's missing fields from Chromium,** such as exclusions, name sources or locations
  of untagged nodes: the transcript would show one engine's data under the other's name.
- **A synthesised Safari tab order:** Safari's tree has no focus walk, and computing one would be a
  guess presented as what the browser exposes.
- **Empty lists for the parts Safari does not supply:** an empty list reads as "the engine reported
  none", which is not what Safari says.
- **Aligning text runs or untagged nodes by position or parent:** a guess that silently pairs the
  wrong nodes when the engines' trees differ, which is the case this view exists to show.
- **Two full transcripts in sequence with cross-references, or the rows plus each engine's indexes
  and tab order:** both push the side-by-side lines apart, which is what the view is for.
- **`--cross-check safari` keeping exit 1 off macOS:** two rules for the same platform check are
  harder to script against than one.
