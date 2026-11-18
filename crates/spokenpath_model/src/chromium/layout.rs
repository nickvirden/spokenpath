//! Reads layout membership and computed `visibility` from the raw `DOMSnapshot.captureSnapshot`
//! response, which the snapshot keeps untyped. Capture requests the computed styles `display`
//! then `visibility`, so `visibility` is the second entry of each layout node's styles.

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::AdaptError;

const ROOT: &str = "payload.dom_snapshot";

/// The index of `visibility` in each layout node's computed styles.
const VISIBILITY: usize = 1;

/// What the DOM snapshot says about one node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LayoutNode {
    /// Whether the node has a layout node.
    pub rendered: bool,
    /// Whether the node's computed `visibility` is `hidden`; absent when the node has no layout
    /// node or no computed `visibility`.
    pub hidden: Option<bool>,
}

/// The DOM snapshot's nodes, by `backendNodeId`.
pub(super) struct Layout {
    nodes: HashMap<u64, LayoutNode>,
}

impl Layout {
    /// The node with this backend id, when the DOM snapshot lists it.
    pub fn node(&self, backend_id: u64) -> Option<LayoutNode> {
        self.nodes.get(&backend_id).copied()
    }
}

pub(super) fn read(snapshot: &Map<String, Value>) -> Result<Layout, AdaptError> {
    let strings = array(snapshot.get("strings"), &format!("{ROOT}.strings"))?
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value.as_str().ok_or_else(|| {
                AdaptError::field(
                    format!("{ROOT}.strings[{index}]"),
                    format!("expected a string, found {value}"),
                )
            })
        })
        .collect::<Result<Vec<&str>, AdaptError>>()?;
    let mut nodes = HashMap::new();
    let documents = array(snapshot.get("documents"), &format!("{ROOT}.documents"))?;
    for (document_index, document) in documents.iter().enumerate() {
        let path = format!("{ROOT}.documents[{document_index}]");
        let document = object(Some(document), &path)?;
        let node_list = object(document.get("nodes"), &format!("{path}.nodes"))?;
        let backend_ids = integers(
            node_list.get("backendNodeId"),
            &format!("{path}.nodes.backendNodeId"),
        )?;
        let layout = object(document.get("layout"), &format!("{path}.layout"))?;
        let node_index = integers(layout.get("nodeIndex"), &format!("{path}.layout.nodeIndex"))?;
        let styles_path = format!("{path}.layout.styles");
        let styles = array(layout.get("styles"), &styles_path)?;
        if styles.len() != node_index.len() {
            return Err(AdaptError::field(
                styles_path,
                format!(
                    "has {} entries, but `layout.nodeIndex` has {}",
                    styles.len(),
                    node_index.len()
                ),
            ));
        }

        let mut document_nodes: Vec<LayoutNode> = vec![
            LayoutNode {
                rendered: false,
                hidden: None,
            };
            backend_ids.len()
        ];
        for (layout_index, (&node, style)) in node_index.iter().zip(styles).enumerate() {
            let Some(entry) = usize::try_from(node)
                .ok()
                .and_then(|node| document_nodes.get_mut(node))
            else {
                return Err(AdaptError::field(
                    format!("{path}.layout.nodeIndex[{layout_index}]"),
                    format!(
                        "is {node}, outside the {} nodes of the document",
                        backend_ids.len()
                    ),
                ));
            };
            let style_path = format!("{styles_path}[{layout_index}]");
            let style = integers(Some(style), &style_path)?;
            let mut visibility = None;
            for (style_index, &string) in style.iter().enumerate() {
                let Some(text) = usize::try_from(string)
                    .ok()
                    .and_then(|string| strings.get(string))
                else {
                    return Err(AdaptError::field(
                        format!("{style_path}[{style_index}]"),
                        format!(
                            "is {string}, outside the {} entries of `strings`",
                            strings.len()
                        ),
                    ));
                };
                if style_index == VISIBILITY {
                    visibility = Some(*text);
                }
            }
            *entry = LayoutNode {
                rendered: true,
                hidden: visibility.map(|visibility| visibility == "hidden"),
            };
        }
        for (node_position, (&backend_id, node)) in
            backend_ids.iter().zip(document_nodes).enumerate()
        {
            let Ok(backend_id) = u64::try_from(backend_id) else {
                return Err(AdaptError::field(
                    format!("{path}.nodes.backendNodeId[{node_position}]"),
                    format!("is {backend_id}, which is not a backend node id"),
                ));
            };
            nodes.entry(backend_id).or_insert(node);
        }
    }
    Ok(Layout { nodes })
}

fn array<'a>(value: Option<&'a Value>, path: &str) -> Result<&'a Vec<Value>, AdaptError> {
    match value {
        Some(Value::Array(items)) => Ok(items),
        Some(other) => Err(AdaptError::field(
            path,
            format!("expected an array, found {}", kind(other)),
        )),
        None => Err(AdaptError::field(path, "expected an array, found nothing")),
    }
}

fn object<'a>(value: Option<&'a Value>, path: &str) -> Result<&'a Map<String, Value>, AdaptError> {
    match value {
        Some(Value::Object(fields)) => Ok(fields),
        Some(other) => Err(AdaptError::field(
            path,
            format!("expected an object, found {}", kind(other)),
        )),
        None => Err(AdaptError::field(path, "expected an object, found nothing")),
    }
}

/// An array of integers. Negative values are kept, so a bounds check can report them.
fn integers(value: Option<&Value>, path: &str) -> Result<Vec<i64>, AdaptError> {
    array(value, path)?
        .iter()
        .enumerate()
        .map(|(index, item)| {
            item.as_i64().ok_or_else(|| {
                AdaptError::field(
                    format!("{path}[{index}]"),
                    format!("expected an integer, found {item}"),
                )
            })
        })
        .collect()
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}
