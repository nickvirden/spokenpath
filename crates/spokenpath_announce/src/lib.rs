//! The ordered transcript of what the browser exposes, built from the engine-neutral model: the
//! reading order, the navigation indexes, the excluded nodes and the tab order, with Layer A
//! ("what the browser exposes") and an optional Layer B ("illustrative speech, approximate") kept
//! apart.
//!
//! [`announce`] is the one entry point. It does no file, network or environment I/O, never opens a
//! path the model names, and never fails: a [`NodeIndex`] that does not resolve in the model leaves
//! that reference absent. The rules follow `docs/decisions/ADR-0001-two-layer-output.md`.
//!
//! # Reading order
//!
//! [`Transcript::nodes`] is the exposed tree in pre-order from [`Model::root`], each node's
//! children in the order the model holds them. That is the engine's accessibility child order,
//! which already follows shadow slotting, `aria-owns` and CSS `reading-flow`. Every exposed node
//! is one item, the root, generic containers and text runs included; nothing is reordered,
//! filtered, merged or deduplicated. Each item has a 1-based `number`, a `depth` (the root is 0)
//! and the number of its `parent`. The walk is iterative and the items are a flat list, so a deep
//! tree cannot overflow the stack.
//!
//! # Layer A
//!
//! `layer_a` records what the model holds for the item and nothing more. For an accessible node:
//! the role token and whether it is `aria` or `internal`, the role origin, the DOM identity, the
//! source location (its class from `SourceClass::as_str`, and for a `span` the path, line, column
//! and start-tag byte range, carried as data only), the name, the description with its
//! provenance, the value, every typed state the engine exposes (absent ones omitted), every other
//! property in engine order with its typed value, and the set position. For a text run: its text
//! and its parts. Strings are the model's raw strings.
//!
//! The name lists every source the engine tried, in its order, each with an outcome: `used` for
//! the winner, `invalid` when the engine marks it invalid, `superseded` when an earlier source
//! took precedence, otherwise `empty`. A related node carries its source location when the model
//! holds that DOM node among its exposed or excluded nodes. An item whose name is absent, or
//! empty after trimming whitespace, has the status `no accessible name` and still lists the
//! sources tried (an empty list when the engine reported none); otherwise the status is `named`.
//!
//! Every value the model marks as supplied by the engine or computed by the tool (role origin,
//! description provenance, set position, a name's hidden-content flag, a hiding ancestor) keeps
//! that marking as `exposed by engine` or `computed by tool`. No other value carries a marking.
//!
//! # Excluded nodes
//!
//! [`Transcript::excluded`] lists every excluded node in DOM order with its DOM identity, source
//! location and every reason (the engine-neutral kind, the engine's reason, and the hiding
//! ancestor with its marking), and its role and name only when the model holds them. Excluded
//! nodes are not items and have no Layer B phrase.
//!
//! # Navigation indexes
//!
//! Each index lists items in reading order by number, with the item's role token and its name or
//! the `no accessible name` status. Membership is by the exposed ARIA role token alone; an
//! engine-internal role or a DOM tag never qualifies an item.
//!
//! - `headings`: `heading`, with the exposed level when there is one.
//! - `landmarks`: [`LANDMARK_ROLES`]: `banner`, `complementary`, `contentinfo`, `form`, `main`,
//!   `navigation`, `region`, `search`.
//! - `links`: `link`.
//! - `form_controls`: [`FORM_CONTROL_ROLES`]: `button`, `checkbox`, `combobox`, `listbox`,
//!   `radio`, `searchbox`, `slider`, `spinbutton`, `switch`, `textbox`.
//!
//! # Tab order
//!
//! [`Transcript::tab_order`] has one entry per Tab press with its 1-based step number and stop: an
//! element (DOM identity, source location, and item number when it is exposed), `left-page`, or
//! an unresolved node by the engine's id. The step the walk marks as its cycle start names the
//! earliest earlier step with the same DOM id (or engine id) as the step it repeats. Repeats of a
//! frame element on consecutive steps are never the cycle start, so they stay ordinary stops.
//!
//! The ending is computed by tool, with one line from [`ENDING_WORDING`] that states only what the
//! Tab presses did:
//!
//! - `truncated`, when the walk stopped at its step limit: "The walk stopped at its limit after
//!   N Tab presses."
//! - `wrap`, when a step repeats an earlier one and focus left the page before it: "Focus left the
//!   page and Tab returned to step M, the normal end of a walk."
//! - `trap`, when a step repeats an earlier one without focus leaving the page: "Tab returned to
//!   step M without focus leaving the page: a focus trap."
//! - `left-page`, when the last step is `left-page`: "Focus left the page after K element stops,
//!   with no return."
//! - `ended`, otherwise, including a walk with no steps: "The walk ended after N steps."
//!
//! # Layer B
//!
//! With [`Options::layer_b`] set (the default), each item's `approx` holds a `template`
//! identifier from [`TEMPLATES`] and one `phrase`, rendered from the item's Layer A record alone.
//! With it unset, no item has an `approx` and the transcript is otherwise identical. Layer B is
//! illustrative speech, approximate: it imitates no specific screen reader. `{role}` is the ARIA
//! role token as exposed; an engine-internal role, `generic`, or no role is unspoken. `{parts}`
//! is the comma-and-space-joined list of the present parts, in order: `level N`, the value, the
//! state words (`checked`, `not checked`, `mixed`; `pressed`, `not pressed`, `mixed`; `expanded`,
//! `collapsed`; `selected`, `disabled`, `required`, `read-only` when true; `invalid` when the
//! invalid token is present and not `false`), and the set position (`P of S`, or `P` when the size
//! is unknown).
//!
//! | Identifier | Pattern | For |
//! |---|---|---|
//! | `text-run` | `{text}` | A text run. |
//! | `named` | `{name}, {role}[, {parts}]` | A named item with a spoken role. |
//! | `unnamed` | `{role}[, {parts}]` | An unnamed item with a spoken role. |
//! | `named-unspoken-role` | `{name}[, {parts}]` | A named item with an unspoken role. |
//! | `unnamed-unspoken-role` | `{parts}` | An unnamed item with an unspoken role; possibly empty. |
//!
//! An unnamed item's phrase holds no name text. A description is never in a phrase.
//!
//! # Terminal escaping
//!
//! Page-controlled text reaches phrases, so every phrase passes through [`escape_for_terminal`]:
//! each control character (C0, DEL, C1, including the CSI introducer U+009B) and each
//! bidirectional embedding, override or isolate control (U+202A to U+202E, U+2066 to U+2069) is
//! replaced with its `\u{..}` escape; every other character, non-ASCII letters included, passes
//! unchanged. Layer A keeps the raw strings; a printer escapes them with the same function.

mod approx;
mod tab_order;
mod transcript;

#[cfg(test)]
mod tests;

pub use approx::{
    Approx, NAMED, NAMED_UNSPOKEN_ROLE, TEMPLATES, TEXT_RUN, Template, UNNAMED,
    UNNAMED_UNSPOKEN_ROLE, escape_for_terminal,
};
pub use tab_order::{ENDING_WORDING, Ending, EndingKind, EndingWording, Stop, TabOrder, TabStep};
pub use transcript::*;

use spokenpath_model::{ExposedNode, Model, NodeIndex};

/// What [`announce`] produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Whether each item gets a Layer B phrase under `approx`.
    pub layer_b: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { layer_b: true }
    }
}

/// The transcript of `model`.
#[must_use]
pub fn announce(model: &Model, options: Options) -> Transcript {
    let sources = DomSources::new(model);
    let mut numbers: Vec<Option<usize>> = vec![None; model.nodes.len()];
    let mut nodes: Vec<Item> = Vec::with_capacity(model.nodes.len());
    let mut indexes = Indexes::default();

    // Each entry is a node to visit with its depth and its parent's item number. Children are
    // pushed in reverse so they pop in the model's order.
    let mut pending: Vec<(NodeIndex, usize, Option<usize>)> = vec![(model.root, 0, None)];
    while let Some((index, depth, parent)) = pending.pop() {
        let (Some(node), Some(slot)) = (model.node(index), numbers.get_mut(index.get())) else {
            continue;
        };
        // An adapter's model is a tree, so this only guards against a node listed twice.
        if slot.is_some() {
            continue;
        }
        let number = nodes.len() + 1;
        *slot = Some(number);

        let layer_a = match node {
            ExposedNode::Accessible(node) => {
                let record = sources.accessible(node);
                indexes.add(number, &record);
                LayerA::Accessible(Box::new(record))
            }
            ExposedNode::TextRun(run) => LayerA::TextRun(DomSources::text_run(run)),
        };
        let approx = options.layer_b.then(|| approx::render(&layer_a));
        nodes.push(Item {
            number,
            depth,
            parent,
            layer_a,
            approx,
        });
        pending.extend(
            node.children()
                .iter()
                .rev()
                .map(|&child| (child, depth + 1, Some(number))),
        );
    }

    let tab_order = tab_order::tab_order(&model.focus, &sources, |node| {
        numbers.get(node.get()).copied().flatten()
    });

    Transcript {
        header: Header {
            engine: model.engine.as_str(),
            engine_version: model.engine_version.clone(),
            tool_version: model.tool_version.clone(),
        },
        nodes,
        indexes,
        excluded: model
            .excluded
            .iter()
            .map(|node| sources.excluded(node))
            .collect(),
        tab_order,
    }
}
