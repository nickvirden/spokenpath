//! The referential-integrity checks the adapter runs before it interprets anything. The snapshot
//! reader checks only each field's shape, so every reference between sections is resolved here,
//! and a dangling or inconsistent one is an error naming the field.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;
use std::hash::Hash;

use spokenpath_snapshot::{ByteRange, ChromiumPayload, Snapshot};

use crate::AdaptError;

/// The lookups that every later step uses, built while checking. Positions are indexes into the
/// snapshot's arrays; ids are the values the snapshot's fields hold.
pub(super) struct Index<'a> {
    pub dom_by_id: HashMap<u32, usize>,
    pub dom_by_backend: HashMap<u64, usize>,
    /// The position of each DOM node's parent, by position.
    pub dom_parent: Vec<Option<usize>>,
    /// Every DOM position in tree pre-order, so parents come before their children.
    pub dom_preorder: Vec<usize>,
    pub span_by_id: HashMap<u32, usize>,
    pub source_by_id: HashMap<u32, usize>,
    pub ax_by_id: HashMap<&'a str, usize>,
    pub ax_root: usize,
}

pub(super) fn check<'a>(
    snapshot: &Snapshot,
    payload: &'a ChromiumPayload,
    source_lengths: &BTreeMap<u32, u64>,
) -> Result<Index<'a>, AdaptError> {
    let source_by_id = unique(snapshot.sources.iter().map(|source| source.id), |index| {
        format!("sources[{index}].id")
    })?;
    let span_by_id = unique(snapshot.spans.iter().map(|span| span.id), |index| {
        format!("spans[{index}].id")
    })?;
    check_spans(snapshot, &source_by_id, source_lengths)?;
    let dom_by_id = unique(payload.dom.iter().map(|node| node.id), |index| {
        format!("payload.dom[{index}].id")
    })?;
    let dom_by_backend = unique(payload.dom.iter().map(|node| node.backend_id), |index| {
        format!("payload.dom[{index}].backend_id")
    })?;
    let (dom_parent, dom_preorder) = check_dom(payload, &dom_by_id, &span_by_id)?;
    check_dom_references(snapshot, payload, &dom_by_id)?;
    let ax_by_id = unique(
        payload.ax.iter().map(|node| node.node_id.as_str()),
        |index| format!("payload.ax[{index}].nodeId"),
    )?;
    let ax_root = check_ax(payload, &ax_by_id)?;
    Ok(Index {
        dom_by_id,
        dom_by_backend,
        dom_parent,
        dom_preorder,
        span_by_id,
        source_by_id,
        ax_by_id,
        ax_root,
    })
}

/// Maps each key to its position, failing at the first key that repeats.
fn unique<K: Hash + Eq + Display>(
    keys: impl Iterator<Item = K>,
    path: impl Fn(usize) -> String,
) -> Result<HashMap<K, usize>, AdaptError> {
    let mut positions = HashMap::new();
    for (position, key) in keys.enumerate() {
        match positions.entry(key) {
            Entry::Occupied(first) => {
                return Err(AdaptError::field(
                    path(position),
                    format!(
                        "repeats {}, which index {} already holds",
                        first.key(),
                        first.get()
                    ),
                ));
            }
            Entry::Vacant(slot) => {
                slot.insert(position);
            }
        }
    }
    Ok(positions)
}

fn check_spans(
    snapshot: &Snapshot,
    source_by_id: &HashMap<u32, usize>,
    source_lengths: &BTreeMap<u32, u64>,
) -> Result<(), AdaptError> {
    for (position, span) in snapshot.spans.iter().enumerate() {
        if !source_by_id.contains_key(&span.source) {
            return Err(AdaptError::field(
                format!("spans[{position}].source"),
                format!("names source {}, which is not in `sources`", span.source),
            ));
        }
        let length = source_lengths.get(&span.source).copied();
        check_range(
            &span.start_tag,
            length,
            span.source,
            &format!("spans[{position}].start_tag"),
        )?;
        if let Some(element) = &span.element {
            check_range(
                element,
                length,
                span.source,
                &format!("spans[{position}].element"),
            )?;
        }
    }
    Ok(())
}

fn check_range(
    range: &ByteRange,
    length: Option<u64>,
    source: u32,
    path: &str,
) -> Result<(), AdaptError> {
    if range.start > range.end {
        return Err(AdaptError::field(
            path,
            format!(
                "starts at byte {}, after its end {}",
                range.start, range.end
            ),
        ));
    }
    if let Some(length) = length
        && range.end > length
    {
        return Err(AdaptError::field(
            path,
            format!(
                "ends at byte {}, past the end of source {source}, which is {length} bytes long",
                range.end
            ),
        ));
    }
    Ok(())
}

/// Checks that the DOM forms one tree whose `parent` and `children` agree, and returns each
/// node's parent position and the pre-order of positions.
fn check_dom(
    payload: &ChromiumPayload,
    dom_by_id: &HashMap<u32, usize>,
    span_by_id: &HashMap<u32, usize>,
) -> Result<(Vec<Option<usize>>, Vec<usize>), AdaptError> {
    let mut parents = Vec::with_capacity(payload.dom.len());
    let mut root = None;
    for (position, node) in payload.dom.iter().enumerate() {
        if let Some(span) = node.span
            && !span_by_id.contains_key(&span)
        {
            return Err(AdaptError::field(
                format!("payload.dom[{position}].span"),
                format!("names span {span}, which is not in `spans`"),
            ));
        }
        match node.parent {
            None => {
                if let Some(first) = root {
                    return Err(AdaptError::field(
                        format!("payload.dom[{position}].parent"),
                        format!(
                            "is absent, but `payload.dom[{first}]` is already the root; the DOM must have one root"
                        ),
                    ));
                }
                root = Some(position);
                parents.push(None);
            }
            Some(parent) => {
                let Some(&parent_position) = dom_by_id.get(&parent) else {
                    return Err(AdaptError::field(
                        format!("payload.dom[{position}].parent"),
                        format!("names node {parent}, which is not in `payload.dom`"),
                    ));
                };
                parents.push(Some(parent_position));
            }
        }
    }
    // Every parent resolves before any child list is compared with it, so a dangling parent is
    // reported as the parent field rather than as a mismatch in the list that names the node.
    for (position, node) in payload.dom.iter().enumerate() {
        for (child_index, child) in node.children.iter().enumerate() {
            let path = || format!("payload.dom[{position}].children[{child_index}]");
            let Some(&child_position) = dom_by_id.get(child) else {
                return Err(AdaptError::field(
                    path(),
                    format!("names node {child}, which is not in `payload.dom`"),
                ));
            };
            let child_parent = payload.dom[child_position].parent;
            if child_parent != Some(node.id) {
                let parent_text = child_parent
                    .map_or_else(|| "no parent".to_owned(), |id| format!("parent {id}"));
                return Err(AdaptError::field(
                    path(),
                    format!(
                        "names node {child}, which has {parent_text}, not node {}",
                        node.id
                    ),
                ));
            }
        }
    }
    let Some(root) = root else {
        return Err(AdaptError::field(
            "payload.dom",
            "has no node without a parent, so it has no root",
        ));
    };

    let mut reached = vec![false; payload.dom.len()];
    reached[root] = true;
    let mut preorder = Vec::with_capacity(payload.dom.len());
    let mut stack = vec![root];
    while let Some(position) = stack.pop() {
        preorder.push(position);
        let children = &payload.dom[position].children;
        for (child_index, child) in children.iter().enumerate().rev() {
            let child_position = dom_by_id[child];
            if reached[child_position] {
                return Err(AdaptError::field(
                    format!("payload.dom[{position}].children[{child_index}]"),
                    format!("names node {child} a second time"),
                ));
            }
            reached[child_position] = true;
            stack.push(child_position);
        }
    }
    if let Some(position) = reached.iter().position(|reached| !reached) {
        return Err(AdaptError::field(
            format!("payload.dom[{position}].parent"),
            format!(
                "node {} is not reached from the root through `children`, so the DOM is not one tree (its parents form a cycle)",
                payload.dom[position].id
            ),
        ));
    }
    Ok((parents, preorder))
}

/// Checks the partial-tree records, diagnostics and focus steps against the DOM.
fn check_dom_references(
    snapshot: &Snapshot,
    payload: &ChromiumPayload,
    dom_by_id: &HashMap<u32, usize>,
) -> Result<(), AdaptError> {
    for (position, forced) in payload.ax_forced.iter().enumerate() {
        let Some(&node) = dom_by_id.get(&forced.node) else {
            return Err(AdaptError::field(
                format!("payload.ax_forced[{position}].node"),
                format!("names node {}, which is not in `payload.dom`", forced.node),
            ));
        };
        let backend_id = payload.dom[node].backend_id;
        if forced.backend_id != backend_id {
            return Err(AdaptError::field(
                format!("payload.ax_forced[{position}].backend_id"),
                format!(
                    "is {}, but node {} has backend id {backend_id}",
                    forced.backend_id, forced.node
                ),
            ));
        }
        if !forced
            .nodes
            .iter()
            .any(|ax| ax.backend_dom_node_id == Some(backend_id))
        {
            return Err(AdaptError::field(
                format!("payload.ax_forced[{position}].nodes"),
                format!("holds no node for backend id {backend_id}, the node it was queried for"),
            ));
        }
    }
    for (position, diagnostic) in snapshot.diagnostics.iter().enumerate() {
        if let Some(node) = diagnostic.node
            && !dom_by_id.contains_key(&node)
        {
            return Err(AdaptError::field(
                format!("diagnostics[{position}].node"),
                format!("names node {node}, which is not in `payload.dom`"),
            ));
        }
    }
    let focus = &payload.focus;
    for (position, step) in focus.steps.iter().enumerate() {
        let Some(node) = step.node else {
            continue;
        };
        let Some(&dom_position) = dom_by_id.get(&node) else {
            return Err(AdaptError::field(
                format!("payload.focus.steps[{position}].node"),
                format!("names node {node}, which is not in `payload.dom`"),
            ));
        };
        let backend_id = payload.dom[dom_position].backend_id;
        if let Some(step_backend) = step.backend_id
            && step_backend != backend_id
        {
            return Err(AdaptError::field(
                format!("payload.focus.steps[{position}].backend_id"),
                format!("is {step_backend}, but node {node} has backend id {backend_id}"),
            ));
        }
    }
    if let Some(cycle_start) = focus.cycle_start
        && usize::try_from(cycle_start).map_or(true, |start| start >= focus.steps.len())
    {
        return Err(AdaptError::field(
            "payload.focus.cycle_start",
            format!(
                "is {cycle_start}, outside the {} steps of the walk",
                focus.steps.len()
            ),
        ));
    }
    Ok(())
}

/// Checks that the accessibility tree has one root, that every `parentId` and `childIds` entry
/// resolves, and that the walk from the root reaches every node exactly once. Returns the root's
/// position.
fn check_ax(
    payload: &ChromiumPayload,
    ax_by_id: &HashMap<&str, usize>,
) -> Result<usize, AdaptError> {
    let mut root = None;
    for (position, node) in payload.ax.iter().enumerate() {
        match &node.parent_id {
            None => {
                if let Some(first) = root {
                    return Err(AdaptError::field(
                        format!("payload.ax[{position}].parentId"),
                        format!(
                            "is absent, but `payload.ax[{first}]` is already the root; the tree must have one root"
                        ),
                    ));
                }
                root = Some(position);
            }
            Some(parent) => {
                if !ax_by_id.contains_key(parent.as_str()) {
                    return Err(AdaptError::field(
                        format!("payload.ax[{position}].parentId"),
                        format!("names node {parent:?}, which is not in `payload.ax`"),
                    ));
                }
            }
        }
        for (child_index, child) in node.child_ids.iter().flatten().enumerate() {
            if !ax_by_id.contains_key(child.as_str()) {
                return Err(AdaptError::field(
                    format!("payload.ax[{position}].childIds[{child_index}]"),
                    format!("names node {child:?}, which is not in `payload.ax`"),
                ));
            }
        }
    }
    let Some(root) = root else {
        return Err(AdaptError::field(
            "payload.ax",
            "has no node without a parentId, so the tree has no root",
        ));
    };

    let mut reached = vec![false; payload.ax.len()];
    reached[root] = true;
    let mut stack = vec![root];
    while let Some(position) = stack.pop() {
        for (child_index, child) in payload.ax[position].child_ids.iter().flatten().enumerate() {
            let child_position = ax_by_id[child.as_str()];
            if reached[child_position] {
                return Err(AdaptError::field(
                    format!("payload.ax[{position}].childIds[{child_index}]"),
                    format!("names node {child:?}, which the walk from the root already reached"),
                ));
            }
            reached[child_position] = true;
            stack.push(child_position);
        }
    }
    if let Some(position) = reached.iter().position(|reached| !reached) {
        return Err(AdaptError::field(
            format!("payload.ax[{position}].nodeId"),
            format!(
                "node {:?} is not reached from the root through `childIds`",
                payload.ax[position].node_id
            ),
        ));
    }
    Ok(root)
}
