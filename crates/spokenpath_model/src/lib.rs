//! The engine-neutral model of what the browser exposes, which a per-engine adapter builds from a
//! snapshot.
//!
//! The model is one typed tree of exposed nodes, the list of nodes the engine leaves out of that
//! tree, the focus walk and the capture's diagnostics. Its types name no engine's protocol
//! structures, so every engine's adapter fills the same types
//! (`docs/decisions/ADR-0002-browser-ground-truth-and-rust-core.md`). The adapters read what the
//! engine exposes and never compute a name or a description text; the core does all
//! interpretation. [`chromium::adapt`] is the adapter for Chromium snapshots. It does no file or
//! network I/O.
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

mod error;
mod model;

pub mod chromium;

pub use error::AdaptError;
pub use model::*;
