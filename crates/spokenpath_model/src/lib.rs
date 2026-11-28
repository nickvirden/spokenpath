//! The engine-neutral model of what the browser exposes, which a per-engine adapter builds from a
//! snapshot.
//!
//! The model is one typed tree of exposed nodes, the list of nodes the engine leaves out of that
//! tree, the focus walk and the capture's diagnostics. Its types name no engine's protocol
//! structures, so every engine's adapter fills the same types
//! (`docs/decisions/ADR-0002-browser-ground-truth-and-rust-core.md`). The adapters read what the
//! engine exposes and never compute a name or a description text; the core does all
//! interpretation. [`chromium::adapt`] is the adapter for Chromium snapshots,
//! [`safari_ax::adapt`] the adapter for snapshots of Safari's macOS accessibility tree, and
//! [`safari_webdriver::answers`] reads Safari's per-span role and label answers from a Safari
//! `WebDriver` snapshot for a cross-check. None does file, network or environment I/O.
//!
//! # Parts an engine does not supply
//!
//! The excluded nodes ([`Model::excluded`]), the focus walk ([`Model::focus`]) and each name's
//! sources ([`Name::sources`]) are each an [`Availability`]: `Available` with what the engine
//! supplied, possibly empty, or `NotAvailable` when the engine has no way to supply the part.
//! The Chromium adapter fills all three; the Safari adapter fills none, because the platform
//! exposes no exclusion reasons, no name provenance and no focus walk.
//!
//! # Values computed by the tool
//!
//! A value the adapter derives, rather than reads from an engine field, is wrapped in
//! [`Supplied`], whose [`SuppliedBy::Tool`] marks it as computed by tool
//! (`docs/decisions/ADR-0001-two-layer-output.md`). Set position, role origin, description
//! provenance, a name's hidden-content flag, and a hiding ancestor found from the DOM or the DOM
//! snapshot are computed by tool; a hiding ancestor the engine names is [`SuppliedBy::Engine`]. A
//! field the engine does not supply and the tool does not compute is absent, never defaulted or
//! guessed.
//!
//! # How the Chromium adapter reads a snapshot
//!
//! The rules follow `docs/decisions/ADR-0012-chromium-protocol-behaviours-and-hidden-content-names.md`,
//! which supersedes `docs/decisions/ADR-0008-chromium-protocol-behaviours.md` and keeps its probe
//! results as evidence.
//!
//! - **Tree.** The exposed tree comes from the full accessibility tree (`payload.ax`) alone, never
//!   from the partial-tree results (`ax_forced`). Every node that is not ignored becomes an
//!   exposed node, except `InlineTextBox` nodes, which are never in the model, and text nodes,
//!   which merge into text runs. The root is always an exposed node. An exposed node's children
//!   are its exposed descendants in `childIds` order; the walk goes through ignored nodes, so the
//!   children of an ignored node take its place in its parent's list.
//! - **Text runs.** Consecutive `StaticText` and `LineBreak` children in one child list merge into
//!   one [`TextRun`]: the `StaticText` names joined in tree order, with `"\n"` for each
//!   `LineBreak`. A lone `StaticText` is a run of one.
//! - **Role.** An ARIA role token (`role.type` is `role`) or an engine-internal role kept as text
//!   (`internalRole`). Its [`RoleOrigin`] is `Author` when the element's `role` attribute holds the
//!   exposed ARIA role as one of its whitespace-separated tokens (ASCII case-insensitive), `Native`
//!   when the node has a DOM element and that test fails, and absent when it has no DOM element.
//! - **Source.** Each node is classified by [`SourceClass`]: a span reference for an element of
//!   origin `source` that has a span, otherwise the element's origin, or `document`, `text`, or
//!   `not-in-dom` for a node whose DOM node the snapshot does not hold (pseudo-element content).
//! - **Name.** The provenance chain lists every `name.sources` entry in order. The winning entry is
//!   the first with a value and no `superseded: true`; entries before it were tried and empty.
//!   The name is from hidden content when a related node of the winning entry is an element the
//!   engine leaves out for a hiding reason (`aria-hidden`, `inert`, not rendered, not visible, or
//!   outside an active modal dialog), since the engine's entry does not say so. An element left
//!   out for another reason, such as a `label` folded into its control, does not count.
//! - **Description.** Chromium does not say where a description came from, so the adapter
//!   attributes it from the element's raw DOM attributes, first match wins: `aria-describedby`
//!   with at least one idref naming an element in the same tree scope (the document, or the
//!   shadow root holding the element); a non-empty `aria-description`; a non-empty `title`, unless
//!   the name's winning entry used `title`. Otherwise the provenance is absent.
//! - **States.** The common states are typed fields of [`States`]; every other property the tree
//!   exposes is kept in order as a [`Property`].
//! - **Set position.** Chromium does not expose it, so the adapter counts, in exposed-tree order,
//!   the `listitem` nodes under the same nearest `list`, the `option` nodes under the same nearest
//!   `listbox`, and the `radio` nodes under the same nearest `radiogroup`. A native
//!   `input type=radio` with no `radiogroup` ancestor is counted with the radios sharing its
//!   non-empty `name` and its nearest `form` ancestor (or none), and alone when it has no `name`.
//!   A valid positive `aria-posinset` or `aria-setsize` replaces the counted value;
//!   `aria-setsize="-1"` leaves the size absent. Excluded nodes are never counted.
//! - **Excluded nodes.** Every DOM element with no exposed node is an [`ExcludedNode`], with the
//!   reasons the engine gives for it in `ax` or `ax_forced`. An element with no node in either, or
//!   one the engine ignores with an empty reason list, is excluded with no reasons. Its role
//!   and name are never filled in from the partial tree's role `none`, from the tag, or from
//!   another engine. The hiding ancestor for `ariaHiddenSubtree` is the engine's related node; for
//!   `inert`, the nearest DOM ancestor carrying `inert`; for `notRendered`, the topmost
//!   ancestor-or-self with no layout node whose parent has one; for `notVisible`, the topmost
//!   ancestor-or-self whose computed `visibility` is `hidden` under a parent whose is not. It is
//!   absent when that node is the element itself or the data the rule needs is missing. DOM
//!   ancestry follows `parent`, so it crosses from a shadow root to its host.
//! - **Integrity.** The adapter checks every reference between the snapshot's sections before it
//!   interprets anything, and reports a dangling or inconsistent one as an [`AdaptError`] naming
//!   the field. Every walk is iterative, so a deep tree cannot overflow the stack.
//!
//! # How the Safari adapter reads a snapshot
//!
//! The rules follow `docs/decisions/ADR-0016-safari-adapter-and-per-engine-transcripts.md`. The
//! adapter reads only the `safari-ax` snapshot; nothing is taken from a Chromium snapshot.
//!
//! - **Integrity.** The span checks the Chromium adapter runs apply to `spans` and `sources`
//!   first. Then the adapter rejects, as an [`AdaptError`] naming the dotted path: a payload with
//!   no nodes (`payload.nodes`); node 0 with a `parent`; any other node without one; a node listed
//!   in a `children` list other than its `parent`'s, or listed more than once; node 0 listed as a
//!   child; and nodes their parents do not reach. Every walk is iterative.
//! - **Tree.** The tree is node 0 and its descendants through `children`, each list in its
//!   captured order, which is Safari's `AXChildren` order. Children are never reordered and
//!   `AXChildrenInNavigationOrder` is never read.
//! - **Text runs.** A node whose `AXRole` is the string `AXStaticText`, with no children, no
//!   `span` and a string `AXValue`, is a [`TextRun`] of one text part with no DOM node and source
//!   `text`, whose text is that value. Adjacent runs are not merged. Every other node is an
//!   accessible node.
//! - **Source.** A node with a `span` is located by it, as in the Chromium adapter. Without one,
//!   an `AXWebArea` is `document`, an `AXStaticText` is `text`, and any other node is `unknown`.
//! - **Role.** A non-string `AXRole` leaves the role absent. The subrole is the `AXSubrole`
//!   string; an absent `AXSubrole`, or the no-value error `-25212`, means none; any other value
//!   makes the pair unmappable. Exactly these pairs are ARIA tokens: `AXHeading` `heading`,
//!   `AXLink` `link`, `AXButton` `button`, `AXCheckBox` `checkbox`, `AXRadioButton` `radio`,
//!   `AXRadioGroup` `radiogroup`, `AXTextField` and `AXTextArea` `textbox`, `AXImage` `image`
//!   (each with no subrole), `AXList`/`AXContentList` `list`, and `AXGroup` with
//!   `AXLandmarkBanner`, `AXLandmarkComplementary`, `AXLandmarkContentInfo`, `AXLandmarkForm`,
//!   `AXLandmarkMain`, `AXLandmarkNavigation`, `AXLandmarkRegion` or `AXLandmarkSearch` as the
//!   landmark role. Every other role is [`Role::Platform`] with the role and subrole text, since
//!   each stands for more than one ARIA role in `WebKit`. Role origin and DOM node are absent.
//! - **Name.** A non-empty string `AXTitle`, otherwise a non-empty string `AXDescription`;
//!   otherwise no name. Its sources are not available, and its winner and hidden-content flag are
//!   absent.
//! - **Description.** A non-empty string `AXHelp`, with no provenance.
//! - **Value.** A non-empty string `AXValue`, except on a text run and when the value fills a
//!   state.
//! - **States.** A boolean `AXEnabled` fills `disabled` as its negation; `AXFocused`,
//!   `AXSelected`, `AXExpanded` and `AXRequired` fill `focused`, `selected`, `expanded` and
//!   `required`; a string `AXInvalid` fills `invalid`. On `AXHeading`, an `AXValue` of 1 to 9 fills
//!   `level`; on `AXCheckBox`, 0, 1 or 2 fills `checked` as false, true or mixed; on
//!   `AXRadioButton`, 0 or 1 fills `checked` as false or true; an integer and a whole-valued
//!   float count alike. Any other value fills nothing. `focusable`, `readonly` and `pressed` stay
//!   absent.
//! - **Properties.** Every `string`, `boolean` or `number` attribute no typed field took is a
//!   [`Property`] in attribute-name order: text, boolean, an integer when the number is an `i64`,
//!   or a number. `AXRole` and `AXSubrole` are always kept. The display-dependent attributes
//!   (`AXPosition`, `AXFrame`, `AXSize`, `_AXPrimaryScreenHeight`, `AXVisibleCharacterRange`,
//!   `AXLayoutCount`) and values of every other type are not in the model.
//! - **Not computed.** Set positions are absent; the excluded nodes and the focus walk are not
//!   available; the diagnostics are the snapshot's.

mod error;
mod model;
mod spans;

pub mod chromium;
pub mod safari_ax;
pub mod safari_webdriver;

pub use error::AdaptError;
pub use model::*;
