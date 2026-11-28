//! The Safari adapter: builds a [`Model`] from a snapshot whose payload is `safari-ax`, Safari's
//! macOS accessibility tree, following the rules in the crate documentation and
//! `docs/decisions/ADR-0016-safari-adapter-and-per-engine-transcripts.md`.

use std::collections::BTreeMap;

use serde_json::Number;
use spokenpath_snapshot::{
    Engine, Payload, SafariAxNode, SafariAxPayload, SafariAxValue, Snapshot,
};

use crate::error::excerpt;
use crate::spans::{self, SpanIndex};
use crate::{
    AccessibleNode, AdaptError, Availability, Description, Diagnostic, ExposedNode, Model, Name,
    NodeIndex, Property, PropertyValue, Role, SourceClass, States, TextPart, TextPartKind, TextRun,
    Tristate,
};

/// The `AXError` a read returns when the attribute has no value, `kAXErrorNoValue`. An `AXSubrole`
/// read that fails with it means the node has no subrole.
const NO_VALUE: i32 = -25212;

/// The attributes whose values depend on the display, the window's position or the number of
/// layouts, so they differ between runs of the same page and are not part of the model.
const DISPLAY_DEPENDENT: [&str; 6] = [
    "AXPosition",
    "AXFrame",
    "AXSize",
    "_AXPrimaryScreenHeight",
    "AXVisibleCharacterRange",
    "AXLayoutCount",
];

/// The `AXRole` and `AXSubrole` pairs that stand for exactly one ARIA role in `WebKit`, with
/// that role. Every other pair stays the platform's text.
const ARIA_ROLES: [(&str, Option<&str>, &str); 18] = [
    ("AXHeading", None, "heading"),
    ("AXLink", None, "link"),
    ("AXButton", None, "button"),
    ("AXCheckBox", None, "checkbox"),
    ("AXRadioButton", None, "radio"),
    ("AXRadioGroup", None, "radiogroup"),
    ("AXTextField", None, "textbox"),
    ("AXTextArea", None, "textbox"),
    ("AXImage", None, "image"),
    ("AXList", Some("AXContentList"), "list"),
    ("AXGroup", Some("AXLandmarkBanner"), "banner"),
    ("AXGroup", Some("AXLandmarkComplementary"), "complementary"),
    ("AXGroup", Some("AXLandmarkContentInfo"), "contentinfo"),
    ("AXGroup", Some("AXLandmarkForm"), "form"),
    ("AXGroup", Some("AXLandmarkMain"), "main"),
    ("AXGroup", Some("AXLandmarkNavigation"), "navigation"),
    ("AXGroup", Some("AXLandmarkRegion"), "region"),
    ("AXGroup", Some("AXLandmarkSearch"), "search"),
];

/// Builds the model of what the browser exposes from a Safari accessibility-tree snapshot.
///
/// `source_lengths` maps a source id to the byte length of that source file, as for
/// [`crate::chromium::adapt`]. The adapter reads no files and nothing from the environment.
///
/// # Errors
///
/// Returns [`AdaptError::UnsupportedEngine`] when the payload is not `safari-ax`, and
/// [`AdaptError::Field`] naming the failing field when a span leaves its source or names no
/// source, the payload has no nodes, or the nodes do not form one tree from node 0.
pub fn adapt(
    snapshot: &Snapshot,
    source_lengths: &BTreeMap<u32, u64>,
) -> Result<Model, AdaptError> {
    let Payload::SafariAx(payload) = &snapshot.payload else {
        return Err(AdaptError::UnsupportedEngine {
            expected: Engine::SafariAx,
            found: snapshot.payload.engine(),
        });
    };
    let spans = spans::check(snapshot, source_lengths)?;
    let order = check_tree(payload)?;
    let mut nodes: Vec<ExposedNode> = Vec::with_capacity(order.len());
    // The model index of each snapshot node, by snapshot position, once it is placed.
    let mut placed: Vec<Option<NodeIndex>> = vec![None; payload.nodes.len()];
    for position in order {
        let node = &payload.nodes[position];
        let parent = node
            .parent
            .and_then(|parent| placed.get(parent as usize).copied().flatten());
        let index = NodeIndex(nodes.len());
        placed[position] = Some(index);
        let exposed = match (parent, text_run(node)) {
            (Some(parent), Some(text)) => ExposedNode::TextRun(TextRun {
                parent,
                text: text.to_owned(),
                parts: vec![TextPart {
                    kind: TextPartKind::Text,
                    dom: None,
                    source: SourceClass::Text,
                }],
            }),
            _ => ExposedNode::Accessible(Box::new(accessible(
                snapshot, &spans, node, position, parent,
            )?)),
        };
        nodes.push(exposed);
        if let Some(parent) = parent
            && let ExposedNode::Accessible(parent) = &mut nodes[parent.0]
        {
            parent.children.push(index);
        }
    }
    Ok(Model {
        engine: Engine::SafariAx,
        engine_version: snapshot.capture.engine_version.clone(),
        tool_version: snapshot.capture.tool_version.clone(),
        engine_build: payload.safari_build.clone(),
        os_version: payload.macos_version.clone(),
        os_build: payload.macos_build.clone(),
        root: NodeIndex(0),
        nodes,
        excluded: Availability::NotAvailable,
        focus: Availability::NotAvailable,
        diagnostics: snapshot
            .diagnostics
            .iter()
            .map(|diagnostic| Diagnostic {
                code: diagnostic.code.clone(),
                message: diagnostic.message.clone(),
                // A Safari snapshot holds no DOM for a diagnostic's node to name.
                dom: None,
            })
            .collect(),
    })
}

/// Checks that the nodes form one tree from node 0 whose `parent` fields agree with the
/// `children` lists, and returns the snapshot positions in pre-order through `children`, each
/// list in its captured order. The walk is iterative, so a deep tree cannot overflow the stack.
fn check_tree(payload: &SafariAxPayload) -> Result<Vec<usize>, AdaptError> {
    let count = payload.nodes.len();
    if count == 0 {
        return Err(AdaptError::field(
            "payload.nodes",
            "holds no nodes, so there is no web area",
        ));
    }
    check_parents(payload)?;

    // Each child list is checked against the `parent` it names before anything is walked, so a
    // node listed by the wrong parent is reported where it is listed.
    let mut listed = vec![false; count];
    for (position, node) in payload.nodes.iter().enumerate() {
        for (child_index, &child) in node.children.iter().enumerate() {
            let path = format!("payload.nodes[{position}].children[{child_index}]");
            let Some(child_node) = payload.nodes.get(child as usize) else {
                return Err(AdaptError::field(
                    path,
                    format!("names node {child}, which is not in `payload.nodes`"),
                ));
            };
            if child == 0 {
                return Err(AdaptError::field(
                    path,
                    "names node 0, the web area, which is the root and no node's child",
                ));
            }
            if child_node.parent != Some(node.id) {
                let parent = child_node.parent.map_or_else(
                    || "no parent".to_owned(),
                    |parent| format!("parent {parent}"),
                );
                return Err(AdaptError::field(
                    path,
                    format!(
                        "names node {child}, which has {parent}, not node {}",
                        node.id
                    ),
                ));
            }
            if std::mem::replace(&mut listed[child as usize], true) {
                return Err(AdaptError::field(
                    path,
                    format!("names node {child} a second time"),
                ));
            }
        }
    }
    if let Some(position) = (1..count).find(|&position| !listed[position]) {
        let parent = payload.nodes[position].parent.unwrap_or_default();
        return Err(AdaptError::field(
            format!("payload.nodes[{position}].parent"),
            format!("names node {parent}, whose `children` do not list node {position}"),
        ));
    }

    let mut reached = vec![false; count];
    reached[0] = true;
    let mut order = Vec::with_capacity(count);
    let mut stack = vec![0_usize];
    while let Some(position) = stack.pop() {
        order.push(position);
        // Reversed, so the stack pops them in the captured order.
        for &child in payload.nodes[position].children.iter().rev() {
            reached[child as usize] = true;
            stack.push(child as usize);
        }
    }
    if let Some(position) = reached.iter().position(|reached| !reached) {
        return Err(AdaptError::field(
            format!("payload.nodes[{position}].parent"),
            format!(
                "node {position} is not reached from node 0 through `children`, so the nodes are not one tree (their parents form a cycle)"
            ),
        ));
    }
    Ok(order)
}

/// Checks each node's `id` and `parent`: node 0 alone has no parent, and every parent names a
/// node.
fn check_parents(payload: &SafariAxPayload) -> Result<(), AdaptError> {
    let count = payload.nodes.len();
    for (position, node) in payload.nodes.iter().enumerate() {
        let path = format!("payload.nodes[{position}]");
        if usize::try_from(node.id).ok() != Some(position) {
            return Err(AdaptError::field(
                format!("{path}.id"),
                format!("is {}, not the node's index {position}", node.id),
            ));
        }
        match (position, node.parent) {
            (0, Some(parent)) => {
                return Err(AdaptError::field(
                    format!("{path}.parent"),
                    format!("is {parent}, but node 0 is the web area, the root, and has no parent"),
                ));
            }
            (0, None) => {}
            (_, None) => {
                return Err(AdaptError::field(
                    format!("{path}.parent"),
                    "is absent, but only node 0, the web area, has no parent",
                ));
            }
            (_, Some(parent)) if parent as usize >= count => {
                return Err(AdaptError::field(
                    format!("{path}.parent"),
                    format!("names node {parent}, which is not in `payload.nodes`"),
                ));
            }
            (_, Some(_)) => {}
        }
    }
    Ok(())
}

/// The attribute's string, when it is a string.
fn string<'a>(node: &'a SafariAxNode, name: &str) -> Option<&'a str> {
    match node.attributes.get(name) {
        Some(SafariAxValue::String { value }) => Some(value),
        _ => None,
    }
}

/// The attribute's string, when it is a string that is not empty.
fn non_empty<'a>(node: &'a SafariAxNode, name: &str) -> Option<&'a str> {
    string(node, name).filter(|value| !value.is_empty())
}

/// The text of a node that is a text run: an `AXStaticText` with no children and no span whose
/// `AXValue` is a string.
fn text_run(node: &SafariAxNode) -> Option<&str> {
    let is_text = string(node, "AXRole") == Some("AXStaticText")
        && node.children.is_empty()
        && node.span.is_none();
    is_text.then(|| string(node, "AXValue")).flatten()
}

/// The number as one of the digits 0 to 9, when it is that integer or a float with exactly that
/// value. The states it fills (a heading level, a checked value) take no other values.
fn digit(number: &Number) -> Option<u8> {
    (0_u8..=9).find(|&digit| match (number.as_u64(), number.as_f64()) {
        (Some(integer), _) => integer == u64::from(digit),
        (None, Some(float)) => float.to_bits() == f64::from(digit).to_bits(),
        (None, None) => false,
    })
}

/// The role of a node, from `AXRole` and `AXSubrole`.
fn role(node: &SafariAxNode) -> Option<Role> {
    let role = string(node, "AXRole")?;
    let subrole = match node.attributes.get("AXSubrole") {
        None | Some(SafariAxValue::Error { code: NO_VALUE }) => Ok(None),
        Some(SafariAxValue::String { value }) => Ok(Some(value.as_str())),
        // A subrole the platform could not report: the pair is unknown, so it maps to no token.
        Some(_) => Err(()),
    };
    let Ok(subrole) = subrole else {
        return Some(Role::Platform {
            role: role.to_owned(),
            subrole: None,
        });
    };
    let token = ARIA_ROLES
        .iter()
        .find(|&&(platform, platform_subrole, _)| platform == role && platform_subrole == subrole)
        .map(|&(_, _, token)| token);
    Some(match token {
        Some(token) => Role::Aria(token.to_owned()),
        None => Role::Platform {
            role: role.to_owned(),
            subrole: subrole.map(str::to_owned),
        },
    })
}

fn accessible(
    snapshot: &Snapshot,
    spans: &SpanIndex,
    node: &SafariAxNode,
    position: usize,
    parent: Option<NodeIndex>,
) -> Result<AccessibleNode, AdaptError> {
    let source = match node.span {
        Some(span) => SourceClass::Span(spans.span_ref(snapshot, span).ok_or_else(|| {
            AdaptError::field(
                format!("payload.nodes[{position}].span"),
                format!("names span {span}, which is not in `spans`"),
            )
        })?),
        None => match string(node, "AXRole") {
            Some("AXWebArea") => SourceClass::Document,
            Some("AXStaticText") => SourceClass::Text,
            _ => SourceClass::Unknown,
        },
    };
    // The attributes a typed field takes, so they are not also kept as properties.
    let mut taken: Vec<&'static str> = Vec::new();
    let name = ["AXTitle", "AXDescription"]
        .into_iter()
        .find_map(|attribute| non_empty(node, attribute).map(|text| (attribute, text)))
        .map(|(attribute, text)| {
            taken.push(attribute);
            Name {
                text: text.to_owned(),
                sources: Availability::NotAvailable,
                winner: None,
                from_hidden_content: None,
            }
        });
    let description = non_empty(node, "AXHelp").map(|text| {
        taken.push("AXHelp");
        Description {
            text: text.to_owned(),
            provenance: None,
        }
    });
    let states = states(node, &mut taken);
    let value = if taken.contains(&"AXValue") {
        None
    } else {
        non_empty(node, "AXValue").map(|value| {
            taken.push("AXValue");
            value.to_owned()
        })
    };
    Ok(AccessibleNode {
        parent,
        children: Vec::new(),
        role: role(node),
        role_origin: None,
        dom: None,
        source,
        name,
        description,
        value,
        states,
        properties: properties(node, position, &taken)?,
        set_position: None,
    })
}

/// The typed states of a node, adding each attribute that fills one to `taken`. A value of a type
/// or range the rule does not name fills nothing.
fn states(node: &SafariAxNode, taken: &mut Vec<&'static str>) -> States {
    let mut states = States::default();
    for (attribute, slot, negate) in [
        ("AXEnabled", &mut states.disabled, true),
        ("AXFocused", &mut states.focused, false),
        ("AXSelected", &mut states.selected, false),
        ("AXExpanded", &mut states.expanded, false),
        ("AXRequired", &mut states.required, false),
    ] {
        if let Some(SafariAxValue::Boolean { value }) = node.attributes.get(attribute) {
            *slot = Some(*value != negate);
            taken.push(attribute);
        }
    }
    if let Some(invalid) = string(node, "AXInvalid") {
        states.invalid = Some(invalid.to_owned());
        taken.push("AXInvalid");
    }
    let number = match node.attributes.get("AXValue") {
        Some(SafariAxValue::Number { value }) => digit(value),
        _ => None,
    };
    let filled = match (string(node, "AXRole"), number) {
        (Some("AXHeading"), Some(level @ 1..=9)) => {
            states.level = Some(u32::from(level));
            true
        }
        (Some("AXCheckBox"), Some(checked @ 0..=2)) => {
            states.checked = Some(match checked {
                0 => Tristate::False,
                1 => Tristate::True,
                _ => Tristate::Mixed,
            });
            true
        }
        (Some("AXRadioButton"), Some(checked @ 0..=1)) => {
            states.checked = Some(if checked == 1 {
                Tristate::True
            } else {
                Tristate::False
            });
            true
        }
        _ => false,
    };
    if filled {
        taken.push("AXValue");
    }
    states
}

/// Every string, boolean or number attribute that no typed field took, in attribute-name order,
/// leaving out the display-dependent ones. `AXRole` and `AXSubrole` are always kept.
fn properties(
    node: &SafariAxNode,
    position: usize,
    taken: &[&str],
) -> Result<Vec<Property>, AdaptError> {
    let mut properties = Vec::new();
    for (name, value) in &node.attributes {
        if taken.contains(&name.as_str()) || DISPLAY_DEPENDENT.contains(&name.as_str()) {
            continue;
        }
        let value = match value {
            SafariAxValue::String { value } => PropertyValue::Text(value.clone()),
            SafariAxValue::Boolean { value } => PropertyValue::Boolean(*value),
            SafariAxValue::Number { value } => match value.as_i64() {
                Some(integer) => PropertyValue::Integer(integer),
                None => PropertyValue::Number(value.as_f64().ok_or_else(|| {
                    AdaptError::field(
                        format!("payload.nodes[{position}].attributes.{name}.value"),
                        format!("is {}, not a number", excerpt(value)),
                    )
                })?),
            },
            _ => continue,
        };
        properties.push(Property {
            name: name.clone(),
            value,
        });
    }
    Ok(properties)
}

#[cfg(test)]
mod tests;
