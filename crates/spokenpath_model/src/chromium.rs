//! The Chromium adapter: builds a [`Model`] from a snapshot whose payload is `chromium`, following
//! the rules in the crate documentation and
//! `docs/decisions/ADR-0012-chromium-protocol-behaviours-and-hidden-content-names.md`.

mod integrity;
mod layout;

use std::collections::{BTreeMap, HashMap};

use serde_json::Value;
use spokenpath_snapshot::{
    AxNode, AxProperty, AxRelatedNode, AxValue, AxValueSource, ChromiumPayload, DomNodeKind,
    Engine, Origin, Payload, Snapshot,
};

use crate::error::excerpt;
use crate::spans;
use crate::{
    AccessibleNode, AdaptError, Availability, Description, DescriptionSource, Diagnostic, DomRef,
    ExcludedNode, ExclusionKind, ExclusionReason, ExposedNode, FocusStop, FocusWalk, Model, Name,
    NameSource, NameSourceKind, NativeSource, NodeIndex, Property, PropertyValue, RelatedNode,
    Role, RoleOrigin, SetPosition, SourceClass, SpanRef, States, Supplied, SuppliedBy, TextPart,
    TextPartKind, TextRun, Tristate,
};
use integrity::Index;
use layout::Layout;

/// Builds the model of what the browser exposes from a Chromium snapshot.
///
/// `source_lengths` maps a source id to the byte length of that source file. When it holds a
/// source's length, every span on that source must end within it; an empty map means no source is
/// available, and only each range's own order is checked. The adapter reads no files.
///
/// # Errors
///
/// Returns [`AdaptError::UnsupportedEngine`] when the payload is not `chromium`, and
/// [`AdaptError::Field`] naming the failing field when a reference between the snapshot's sections
/// resolves to nothing or contradicts another field, a span leaves its source, the DOM snapshot is
/// malformed, or a value the model types has an unexpected type.
pub fn adapt(
    snapshot: &Snapshot,
    source_lengths: &BTreeMap<u32, u64>,
) -> Result<Model, AdaptError> {
    let Payload::Chromium(payload) = &snapshot.payload else {
        return Err(AdaptError::UnsupportedEngine {
            expected: Engine::Chromium,
            found: snapshot.payload.engine(),
        });
    };
    let index = integrity::check(snapshot, payload, source_lengths)?;
    let layout = payload
        .dom_snapshot
        .as_ref()
        .map(layout::read)
        .transpose()?;
    let adapter = Adapter::new(snapshot, payload, index, layout.as_ref());
    adapter.run()
}

/// What the adapter knows about the snapshot once its references are checked.
struct Adapter<'a> {
    snapshot: &'a Snapshot,
    payload: &'a ChromiumPayload,
    index: Index<'a>,
    dom: DomFacts<'a>,
    /// Whether each DOM position has a node in the exposed tree, by position.
    exposed: Vec<bool>,
    /// Whether each DOM position is an excluded element with a hiding reason, by position; filled
    /// once the excluded nodes are known.
    hidden: Vec<bool>,
}

/// Facts about each DOM node that depend on its ancestors, computed once, top-down.
struct DomFacts<'a> {
    /// The tree scope each node belongs to: the position of its document or shadow root.
    scope: Vec<usize>,
    /// The nearest strict ancestor that is a `form` element.
    form: Vec<Option<usize>>,
    /// The nearest strict ancestor carrying the `inert` attribute.
    inert: Vec<Option<usize>>,
    /// The topmost ancestor-or-self with no layout node whose parent has one.
    not_rendered_top: Vec<Option<usize>>,
    /// The topmost ancestor-or-self whose computed `visibility` is `hidden` under a parent whose
    /// is not.
    not_visible_top: Vec<Option<usize>>,
    /// Elements by tree scope and `id` attribute, the first in tree order winning.
    ids: HashMap<(usize, &'a str), usize>,
}

/// How the tree walk treats one accessibility node.
enum Walk {
    /// Never in the model.
    InlineTextBox,
    /// Left out; its children take its place.
    Ignored,
    /// Merged into a text run.
    Text(TextPartKind),
    /// An exposed node.
    Exposed,
}

impl<'a> Adapter<'a> {
    fn new(
        snapshot: &'a Snapshot,
        payload: &'a ChromiumPayload,
        index: Index<'a>,
        layout: Option<&Layout>,
    ) -> Self {
        let dom = DomFacts::new(payload, &index, layout);
        let mut exposed = vec![false; payload.dom.len()];
        for node in &payload.ax {
            if !node.ignored
                && let Some(position) = backend_position(&index, node.backend_dom_node_id)
            {
                exposed[position] = true;
            }
        }
        Adapter {
            snapshot,
            payload,
            index,
            dom,
            exposed,
            hidden: Vec::new(),
        }
    }

    fn run(mut self) -> Result<Model, AdaptError> {
        let excluded = self.excluded_nodes();
        self.hidden = vec![false; self.payload.dom.len()];
        for node in &excluded {
            if node.reasons.iter().any(|reason| reason.kind.hides()) {
                self.hidden[self.index.dom_by_id[&node.dom.dom_id]] = true;
            }
        }
        let mut tree = self.tree()?;
        self.set_positions(&mut tree);
        let focus = self.focus(&tree.dom_to_node);
        let diagnostics = self
            .snapshot
            .diagnostics
            .iter()
            .map(|diagnostic| Diagnostic {
                code: diagnostic.code.clone(),
                message: diagnostic.message.clone(),
                dom: diagnostic
                    .node
                    .map(|id| self.dom_ref(self.index.dom_by_id[&id])),
            })
            .collect();
        Ok(Model {
            engine: Engine::Chromium,
            engine_version: self.snapshot.capture.engine_version.clone(),
            tool_version: self.snapshot.capture.tool_version.clone(),
            engine_build: None,
            os_version: None,
            os_build: None,
            root: NodeIndex(0),
            nodes: tree.nodes,
            excluded: Availability::Available(excluded),
            focus: Availability::Available(focus),
            diagnostics,
        })
    }

    fn dom_ref(&self, position: usize) -> DomRef {
        let node = &self.payload.dom[position];
        DomRef {
            dom_id: node.id,
            tag: node.tag.clone(),
            id: self.attribute(position, "id").map(str::to_owned),
        }
    }

    fn attribute(&self, position: usize, name: &str) -> Option<&'a str> {
        self.payload.dom[position]
            .attributes
            .iter()
            .find(|attribute| attribute.name == name)
            .map(|attribute| attribute.value.as_str())
    }

    fn is_element(&self, position: usize) -> bool {
        self.payload.dom[position].kind == DomNodeKind::Element
    }

    fn is_excluded(&self, position: usize) -> bool {
        self.is_element(position) && !self.exposed[position]
    }

    fn source_class(&self, position: Option<usize>) -> SourceClass {
        let Some(position) = position else {
            return SourceClass::NotInDom;
        };
        let node = &self.payload.dom[position];
        match node.kind {
            DomNodeKind::Document => SourceClass::Document,
            DomNodeKind::Text => SourceClass::Text,
            DomNodeKind::Element => match node.origin {
                // An element of origin `source` without a span has no location to report.
                Some(Origin::Source) => node
                    .span
                    .and_then(|span| self.span_ref(span))
                    .map_or(SourceClass::Unknown, SourceClass::Span),
                Some(Origin::Implied) => SourceClass::Implied,
                Some(Origin::Script) => SourceClass::Script,
                Some(Origin::Library) => SourceClass::Library,
                Some(Origin::Unknown) | None => SourceClass::Unknown,
            },
            _ => SourceClass::Unknown,
        }
    }

    /// The span's location; present for every span id the integrity check resolved.
    fn span_ref(&self, span_id: u32) -> Option<SpanRef> {
        spans::span_ref(
            self.snapshot,
            &self.index.span_by_id,
            &self.index.source_by_id,
            span_id,
        )
    }

    fn related_node(&self, related: &AxRelatedNode) -> RelatedNode {
        RelatedNode {
            dom: backend_position(&self.index, Some(related.backend_dom_node_id))
                .map(|position| self.dom_ref(position)),
            idref: related.idref.clone(),
            text: related.text.clone(),
        }
    }

    fn excluded_nodes(&self) -> Vec<ExcludedNode> {
        let mut ignored_ax: HashMap<usize, usize> = HashMap::new();
        for (ax_position, node) in self.payload.ax.iter().enumerate() {
            if node.ignored
                && let Some(position) = backend_position(&self.index, node.backend_dom_node_id)
            {
                ignored_ax.entry(position).or_insert(ax_position);
            }
        }
        let mut forced: HashMap<usize, usize> = HashMap::new();
        for (forced_position, record) in self.payload.ax_forced.iter().enumerate() {
            forced
                .entry(self.index.dom_by_id[&record.node])
                .or_insert(forced_position);
        }

        let mut excluded = Vec::new();
        for &position in &self.index.dom_preorder {
            if !self.is_excluded(position) {
                continue;
            }
            let reasons_node: Option<&AxNode> =
                if let Some(&ax_position) = ignored_ax.get(&position) {
                    Some(&self.payload.ax[ax_position])
                } else if let Some(&forced_position) = forced.get(&position) {
                    let record = &self.payload.ax_forced[forced_position];
                    // Present: the integrity check requires the queried node in every record.
                    record
                        .nodes
                        .iter()
                        .find(|node| node.backend_dom_node_id == Some(record.backend_id))
                } else {
                    None
                };
            let reasons = reasons_node.map_or_else(Vec::new, |node| self.reasons(node, position));
            excluded.push(ExcludedNode {
                dom: self.dom_ref(position),
                source: self.source_class(Some(position)),
                reasons,
                role: None,
                name: None,
            });
        }
        excluded
    }

    fn reasons(&self, node: &AxNode, position: usize) -> Vec<ExclusionReason> {
        node.ignored_reasons
            .iter()
            .flatten()
            .map(|reason| {
                let kind = exclusion_kind(&reason.name);
                ExclusionReason {
                    kind,
                    engine_reason: reason.name.clone(),
                    hiding_ancestor: self.hiding_ancestor(reason, kind, position),
                }
            })
            .collect()
    }

    fn hiding_ancestor(
        &self,
        reason: &AxProperty,
        kind: ExclusionKind,
        position: usize,
    ) -> Option<Supplied<DomRef>> {
        let (ancestor, by) = match kind {
            ExclusionKind::AriaHidden if reason.name == "ariaHiddenSubtree" => (
                reason
                    .value
                    .related_nodes
                    .iter()
                    .flatten()
                    .next()
                    .and_then(|related| {
                        backend_position(&self.index, Some(related.backend_dom_node_id))
                    }),
                SuppliedBy::Engine,
            ),
            ExclusionKind::Inert => (self.dom.inert[position], SuppliedBy::Tool),
            ExclusionKind::NotRendered => (self.dom.not_rendered_top[position], SuppliedBy::Tool),
            ExclusionKind::NotVisible => (self.dom.not_visible_top[position], SuppliedBy::Tool),
            _ => (None, SuppliedBy::Tool),
        };
        ancestor
            .filter(|&ancestor| ancestor != position)
            .map(|ancestor| Supplied {
                value: self.dom_ref(ancestor),
                by,
            })
    }

    fn tree(&self) -> Result<Tree, AdaptError> {
        let mut tree = Tree {
            nodes: Vec::new(),
            node_dom: Vec::new(),
            dom_to_node: HashMap::new(),
        };
        let root = self.index.ax_root;
        let root_index = tree.push_accessible(self.accessible(root, None)?, self.ax_dom(root));
        let mut stack: Vec<(usize, NodeIndex)> = Vec::new();
        self.push_children(&mut stack, root, root_index);
        while let Some((ax_position, parent)) = stack.pop() {
            let node = &self.payload.ax[ax_position];
            let children_parent = match walk(node) {
                Walk::InlineTextBox | Walk::Ignored => parent,
                Walk::Text(kind) => {
                    let text = match kind {
                        TextPartKind::Text => node
                            .name
                            .as_ref()
                            .map(|name| {
                                scalar_text(
                                    name.value.as_ref(),
                                    &format!("payload.ax[{ax_position}].name.value"),
                                )
                            })
                            .transpose()?
                            .flatten()
                            .unwrap_or_default(),
                        TextPartKind::LineBreak => "\n".to_owned(),
                    };
                    let position = self.ax_dom(ax_position);
                    let part = TextPart {
                        kind,
                        dom: position.map(|position| self.dom_ref(position)),
                        source: self.source_class(position),
                    };
                    tree.push_text(parent, &text, part);
                    parent
                }
                Walk::Exposed => {
                    let accessible = self.accessible(ax_position, Some(parent))?;
                    let index = tree.push_accessible(accessible, self.ax_dom(ax_position));
                    tree.attach(parent, index);
                    index
                }
            };
            self.push_children(&mut stack, ax_position, children_parent);
        }
        Ok(tree)
    }

    fn push_children(
        &self,
        stack: &mut Vec<(usize, NodeIndex)>,
        ax_position: usize,
        parent: NodeIndex,
    ) {
        let children = self.payload.ax[ax_position].child_ids.iter().flatten();
        // Reversed, so the stack pops them in `childIds` order.
        for child in children.rev() {
            stack.push((self.index.ax_by_id[child.as_str()], parent));
        }
    }

    fn ax_dom(&self, ax_position: usize) -> Option<usize> {
        backend_position(
            &self.index,
            self.payload.ax[ax_position].backend_dom_node_id,
        )
    }

    fn accessible(
        &self,
        ax_position: usize,
        parent: Option<NodeIndex>,
    ) -> Result<AccessibleNode, AdaptError> {
        let node = &self.payload.ax[ax_position];
        let path = format!("payload.ax[{ax_position}]");
        let position = self.ax_dom(ax_position);
        let role = node
            .role
            .as_ref()
            .map(|role| read_role(role, &format!("{path}.role")))
            .transpose()?;
        let element = position.filter(|&position| self.is_element(position));
        let role_origin = element.map(|element| {
            let author = role.as_ref().and_then(Role::aria).is_some_and(|role| {
                self.attribute(element, "role").is_some_and(|tokens| {
                    tokens
                        .split_ascii_whitespace()
                        .any(|token| token.eq_ignore_ascii_case(role))
                })
            });
            Supplied::tool(if author {
                RoleOrigin::Author
            } else {
                RoleOrigin::Native
            })
        });
        let name = node
            .name
            .as_ref()
            .map(|name| self.name(name, &format!("{path}.name")))
            .transpose()?;
        let description = match &node.description {
            Some(description) => scalar_text(
                description.value.as_ref(),
                &format!("{path}.description.value"),
            )?
            .map(|text| Description {
                text,
                provenance: element
                    .and_then(|element| self.description_source(element, name.as_ref())),
            }),
            None => None,
        };
        let value = match &node.value {
            Some(value) => scalar_text(value.value.as_ref(), &format!("{path}.value.value"))?,
            None => None,
        };
        let (states, properties) = read_properties(self, node, &path)?;
        Ok(AccessibleNode {
            parent,
            children: Vec::new(),
            role,
            role_origin,
            dom: position.map(|position| self.dom_ref(position)),
            source: self.source_class(position),
            name,
            description,
            value,
            states,
            properties,
            set_position: None,
        })
    }

    fn name(&self, name: &AxValue, path: &str) -> Result<Name, AdaptError> {
        // A name the engine sends without a value reads as empty text.
        let text = scalar_text(name.value.as_ref(), &format!("{path}.value"))?.unwrap_or_default();
        let sources = name
            .sources
            .iter()
            .flatten()
            .enumerate()
            .map(|(index, source)| self.name_source(source, &format!("{path}.sources[{index}]")))
            .collect::<Result<Vec<NameSource>, AdaptError>>()?;
        // A source has a value when the engine sends its `value` object, even one with no text,
        // such as a bare node list.
        let winner = name
            .sources
            .iter()
            .flatten()
            .position(|source| source.value.is_some() && source.superseded != Some(true));
        let from_hidden_content = winner
            .map(|winner| &sources[winner].related_nodes)
            .filter(|related| !related.is_empty())
            .map(|related| {
                Supplied::tool(related.iter().any(|related| {
                    related
                        .dom
                        .as_ref()
                        .is_some_and(|dom| self.hidden[self.index.dom_by_id[&dom.dom_id]])
                }))
            });
        Ok(Name {
            text,
            sources: Availability::Available(sources),
            winner,
            from_hidden_content,
        })
    }

    fn name_source(&self, source: &AxValueSource, path: &str) -> Result<NameSource, AdaptError> {
        let value = match &source.value {
            Some(value) => scalar_text(value.value.as_ref(), &format!("{path}.value.value"))?,
            None => None,
        };
        let attribute_value = match &source.attribute_value {
            Some(value) => scalar_text(
                value.value.as_ref(),
                &format!("{path}.attributeValue.value"),
            )?,
            None => None,
        };
        let related_nodes = [
            &source.value,
            &source.attribute_value,
            &source.native_source_value,
        ]
        .into_iter()
        .flatten()
        .flat_map(|value| value.related_nodes.iter().flatten())
        .map(|related| self.related_node(related))
        .collect();
        let invalid = match source.extra.get("invalid") {
            None => false,
            Some(Value::Bool(invalid)) => *invalid,
            Some(other) => {
                return Err(AdaptError::field(
                    format!("{path}.invalid"),
                    format!("expected a boolean, found {}", excerpt(other)),
                ));
            }
        };
        let invalid_reason = match source.extra.get("invalidReason") {
            None => None,
            Some(Value::String(reason)) => Some(reason.clone()),
            Some(other) => {
                return Err(AdaptError::field(
                    format!("{path}.invalidReason"),
                    format!("expected a string, found {}", excerpt(other)),
                ));
            }
        };
        Ok(NameSource {
            kind: name_source_kind(&source.source_type),
            attribute: source.attribute.clone(),
            attribute_value,
            native_source: source.native_source.as_deref().map(native_source),
            value,
            related_nodes,
            superseded: source.superseded == Some(true),
            invalid,
            invalid_reason,
        })
    }

    fn description_source(
        &self,
        element: usize,
        name: Option<&Name>,
    ) -> Option<Supplied<DescriptionSource>> {
        if let Some(idrefs) = self.attribute(element, "aria-describedby") {
            let scope = self.dom.scope[element];
            let resolved: Vec<DomRef> = idrefs
                .split_ascii_whitespace()
                .filter_map(|idref| self.dom.ids.get(&(scope, idref)))
                .map(|&target| self.dom_ref(target))
                .collect();
            if !resolved.is_empty() {
                return Some(Supplied::tool(DescriptionSource::AriaDescribedby(resolved)));
            }
        }
        let non_empty = |name| {
            self.attribute(element, name)
                .is_some_and(|value| !value.trim_ascii().is_empty())
        };
        if non_empty("aria-description") {
            return Some(Supplied::tool(DescriptionSource::AriaDescription));
        }
        let name_used_title = name.and_then(Name::winning_source).is_some_and(|source| {
            source.attribute.as_deref() == Some("title")
                || source.native_source == Some(NativeSource::Title)
        });
        if non_empty("title") && !name_used_title {
            return Some(Supplied::tool(DescriptionSource::Title));
        }
        None
    }

    fn set_positions(&self, tree: &mut Tree) {
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        enum Container {
            List,
            Listbox,
            Radiogroup,
        }
        #[derive(PartialEq, Eq, Hash)]
        enum SetKey<'k> {
            Container(Container, NodeIndex),
            NamedRadios { name: &'k str, form: Option<usize> },
            Alone(NodeIndex),
        }

        let aria_role = |node: &ExposedNode| {
            node.as_accessible()
                .and_then(|node| node.role.as_ref())
                .and_then(Role::aria)
                .map(str::to_owned)
        };
        // The nearest list, listbox and radiogroup ancestor of each node; parents precede their
        // children in the arena, so one forward pass fills it.
        let mut nearest: Vec<[Option<NodeIndex>; 3]> = vec![[None; 3]; tree.nodes.len()];
        for position in 0..tree.nodes.len() {
            let Some(parent) = tree.nodes[position].parent() else {
                continue;
            };
            let mut ancestors = nearest[parent.0];
            match aria_role(&tree.nodes[parent.0]).as_deref() {
                Some("list") => ancestors[0] = Some(parent),
                Some("listbox") => ancestors[1] = Some(parent),
                Some("radiogroup") => ancestors[2] = Some(parent),
                _ => {}
            }
            nearest[position] = ancestors;
        }

        let mut sets: HashMap<SetKey, Vec<NodeIndex>> = HashMap::new();
        let mut membership: Vec<(NodeIndex, usize)> = Vec::new();
        for position in 0..tree.nodes.len() {
            let index = NodeIndex(position);
            let key = match aria_role(&tree.nodes[position]).as_deref() {
                Some("listitem") => {
                    nearest[position][0].map(|list| SetKey::Container(Container::List, list))
                }
                Some("option") => nearest[position][1]
                    .map(|listbox| SetKey::Container(Container::Listbox, listbox)),
                Some("radio") => nearest[position][2]
                    .map(|group| SetKey::Container(Container::Radiogroup, group))
                    .or_else(|| {
                        let element = tree.node_dom[position]?;
                        self.is_native_radio(element).then(|| {
                            match self
                                .attribute(element, "name")
                                .filter(|name| !name.is_empty())
                            {
                                Some(name) => SetKey::NamedRadios {
                                    name,
                                    form: self.dom.form[element],
                                },
                                None => SetKey::Alone(index),
                            }
                        })
                    }),
                _ => None,
            };
            if let Some(key) = key {
                let members = sets.entry(key).or_default();
                membership.push((index, members.len()));
                members.push(index);
            }
        }
        let mut size_of: HashMap<NodeIndex, usize> = HashMap::new();
        for members in sets.values() {
            for &member in members {
                size_of.insert(member, members.len());
            }
        }

        for (index, rank) in membership {
            let element = tree.node_dom[index.0];
            let author = |name| element.and_then(|element| self.attribute(element, name));
            let position = author("aria-posinset")
                .and_then(positive_integer)
                .unwrap_or_else(|| count(rank + 1));
            let size = match author("aria-setsize") {
                Some(value) if value.trim_ascii() == "-1" => None,
                value => Some(
                    value
                        .and_then(positive_integer)
                        .unwrap_or_else(|| count(size_of[&index])),
                ),
            };
            if let ExposedNode::Accessible(node) = &mut tree.nodes[index.0] {
                node.set_position = Some(Supplied::tool(SetPosition { position, size }));
            }
        }
    }

    fn is_native_radio(&self, element: usize) -> bool {
        self.payload.dom[element]
            .tag
            .as_deref()
            .is_some_and(|tag| tag.eq_ignore_ascii_case("input"))
            && self
                .attribute(element, "type")
                .is_some_and(|kind| kind.eq_ignore_ascii_case("radio"))
    }

    fn focus(&self, dom_to_node: &HashMap<usize, NodeIndex>) -> FocusWalk {
        let focus = &self.payload.focus;
        let steps = focus
            .steps
            .iter()
            .map(|step| match (step.node, step.backend_id) {
                (Some(node), _) => {
                    let position = self.index.dom_by_id[&node];
                    FocusStop::Element {
                        dom: self.dom_ref(position),
                        node: dom_to_node.get(&position).copied(),
                    }
                }
                (None, Some(backend_id)) => FocusStop::Unresolved { backend_id },
                (None, None) => FocusStop::LeftPage,
            })
            .collect();
        FocusWalk {
            steps,
            truncated: focus.truncated,
            cycle_start: focus.cycle_start,
        }
    }
}

/// The exposed tree while it is built.
struct Tree {
    nodes: Vec<ExposedNode>,
    /// The DOM position of each exposed node, by arena position.
    node_dom: Vec<Option<usize>>,
    /// The first accessible node of each DOM position.
    dom_to_node: HashMap<usize, NodeIndex>,
}

impl Tree {
    fn push_accessible(&mut self, node: AccessibleNode, position: Option<usize>) -> NodeIndex {
        let index = NodeIndex(self.nodes.len());
        self.nodes.push(ExposedNode::Accessible(Box::new(node)));
        self.node_dom.push(position);
        if let Some(position) = position {
            self.dom_to_node.entry(position).or_insert(index);
        }
        index
    }

    fn attach(&mut self, parent: NodeIndex, child: NodeIndex) {
        if let ExposedNode::Accessible(node) = &mut self.nodes[parent.0] {
            node.children.push(child);
        }
    }

    /// Adds a text part to `parent`'s child list, extending the run that ends the list when there
    /// is one, so adjacent text merges.
    fn push_text(&mut self, parent: NodeIndex, text: &str, part: TextPart) {
        let last = self.nodes[parent.0].children().last().copied();
        if let Some(last) = last
            && let ExposedNode::TextRun(run) = &mut self.nodes[last.0]
        {
            run.text.push_str(text);
            run.parts.push(part);
            return;
        }
        let index = NodeIndex(self.nodes.len());
        self.nodes.push(ExposedNode::TextRun(TextRun {
            parent,
            text: text.to_owned(),
            parts: vec![part],
        }));
        self.node_dom.push(None);
        self.attach(parent, index);
    }
}

impl<'a> DomFacts<'a> {
    fn new(payload: &'a ChromiumPayload, index: &Index<'_>, layout: Option<&Layout>) -> Self {
        let count = payload.dom.len();
        let mut facts = DomFacts {
            scope: vec![0; count],
            form: vec![None; count],
            inert: vec![None; count],
            not_rendered_top: vec![None; count],
            not_visible_top: vec![None; count],
            ids: HashMap::new(),
        };
        // The nearest strict ancestor that is not a document fragment, so a shadow root's
        // children take their layout parent from the host.
        let mut style_parent: Vec<Option<usize>> = vec![None; count];
        let layout_of = |position: usize| {
            layout.and_then(|layout| layout.node(payload.dom[position].backend_id))
        };
        for &position in &index.dom_preorder {
            let node = &payload.dom[position];
            let parent = index.dom_parent[position];
            facts.scope[position] = match (node.kind, parent) {
                (DomNodeKind::Document | DomNodeKind::DocumentFragment, _) | (_, None) => position,
                (_, Some(parent)) => facts.scope[parent],
            };
            if let Some(parent) = parent {
                let parent_node = &payload.dom[parent];
                let parent_is_form = parent_node.kind == DomNodeKind::Element
                    && parent_node
                        .tag
                        .as_deref()
                        .is_some_and(|tag| tag.eq_ignore_ascii_case("form"));
                facts.form[position] = if parent_is_form {
                    Some(parent)
                } else {
                    facts.form[parent]
                };
                let parent_is_inert = parent_node
                    .attributes
                    .iter()
                    .any(|attribute| attribute.name == "inert");
                facts.inert[position] = if parent_is_inert {
                    Some(parent)
                } else {
                    facts.inert[parent]
                };
                style_parent[position] = if parent_node.kind == DomNodeKind::DocumentFragment {
                    style_parent[parent]
                } else {
                    Some(parent)
                };
            }
            if node.kind == DomNodeKind::Element
                && let Some(id) = node
                    .attributes
                    .iter()
                    .find(|attribute| attribute.name == "id")
            {
                facts
                    .ids
                    .entry((facts.scope[position], id.value.as_str()))
                    .or_insert(position);
            }

            let own = layout_of(position);
            let above = style_parent[position].map(|parent| (parent, layout_of(parent)));
            if own.is_some_and(|own| !own.rendered) {
                facts.not_rendered_top[position] = match above {
                    Some((_, Some(parent))) if parent.rendered => Some(position),
                    Some((parent, Some(_))) => facts.not_rendered_top[parent],
                    _ => None,
                };
            }
            if own.and_then(|own| own.hidden) == Some(true) {
                facts.not_visible_top[position] = match above {
                    Some((parent, Some(layout))) => match layout.hidden {
                        Some(true) => facts.not_visible_top[parent],
                        Some(false) => Some(position),
                        None => None,
                    },
                    _ => None,
                };
            }
        }
        facts
    }
}

fn backend_position(index: &Index<'_>, backend_id: Option<u64>) -> Option<usize> {
    backend_id.and_then(|backend_id| index.dom_by_backend.get(&backend_id).copied())
}

fn walk(node: &AxNode) -> Walk {
    let role = node
        .role
        .as_ref()
        .and_then(|role| role.value.as_ref())
        .and_then(Value::as_str);
    match role {
        Some("InlineTextBox") => Walk::InlineTextBox,
        _ if node.ignored => Walk::Ignored,
        Some("StaticText") => Walk::Text(TextPartKind::Text),
        Some("LineBreak") => Walk::Text(TextPartKind::LineBreak),
        _ => Walk::Exposed,
    }
}

fn read_role(role: &AxValue, path: &str) -> Result<Role, AdaptError> {
    let Some(Value::String(text)) = &role.value else {
        return Err(AdaptError::field(
            format!("{path}.value"),
            format!(
                "expected a string, found {}",
                json_or_nothing(role.value.as_ref())
            ),
        ));
    };
    match role.value_type.as_str() {
        "role" => Ok(Role::Aria(text.clone())),
        "internalRole" => Ok(Role::Internal(text.clone())),
        other => Err(AdaptError::field(
            format!("{path}.type"),
            format!(
                "expected `role` or `internalRole`, found {}",
                excerpt(&format!("{other:?}"))
            ),
        )),
    }
}

fn read_properties(
    adapter: &Adapter<'_>,
    node: &AxNode,
    path: &str,
) -> Result<(States, Vec<Property>), AdaptError> {
    let mut states = States::default();
    let mut properties = Vec::new();
    for (index, property) in node.properties.iter().flatten().enumerate() {
        let path = format!("{path}.properties[{index}].value");
        let value = property.value.value.as_ref();
        let name = property.name.as_str();
        match name {
            "checked" => states.checked = Some(tristate(value, name, &path)?),
            "pressed" => states.pressed = Some(tristate(value, name, &path)?),
            "expanded" => states.expanded = Some(boolean(value, name, &path)?),
            "selected" => states.selected = Some(boolean(value, name, &path)?),
            "disabled" => states.disabled = Some(boolean(value, name, &path)?),
            "required" => states.required = Some(boolean(value, name, &path)?),
            "readonly" => states.readonly = Some(boolean(value, name, &path)?),
            "focusable" => states.focusable = Some(boolean(value, name, &path)?),
            "focused" => states.focused = Some(boolean(value, name, &path)?),
            "invalid" => match value {
                Some(Value::String(token)) => states.invalid = Some(token.clone()),
                other => return Err(unexpected(&path, name, "a token", other)),
            },
            "level" => {
                states.level = Some(
                    value
                        .and_then(Value::as_u64)
                        .and_then(|level| u32::try_from(level).ok())
                        .ok_or_else(|| unexpected(&path, name, "a non-negative integer", value))?,
                );
            }
            _ => properties.push(Property {
                name: property.name.clone(),
                value: property_value(adapter, &property.value, &path)?,
            }),
        }
    }
    Ok((states, properties))
}

fn tristate(value: Option<&Value>, name: &str, path: &str) -> Result<Tristate, AdaptError> {
    match value {
        Some(Value::Bool(true)) => Ok(Tristate::True),
        Some(Value::Bool(false)) => Ok(Tristate::False),
        Some(Value::String(token)) if token == "true" => Ok(Tristate::True),
        Some(Value::String(token)) if token == "false" => Ok(Tristate::False),
        Some(Value::String(token)) if token == "mixed" => Ok(Tristate::Mixed),
        other => Err(unexpected(path, name, "true, false or mixed", other)),
    }
}

fn boolean(value: Option<&Value>, name: &str, path: &str) -> Result<bool, AdaptError> {
    match value {
        Some(Value::Bool(value)) => Ok(*value),
        other => Err(unexpected(path, name, "a boolean", other)),
    }
}

fn unexpected(path: &str, name: &str, expected: &str, found: Option<&Value>) -> AdaptError {
    AdaptError::field(
        path,
        format!(
            "expected {expected} for `{name}`, found {}",
            json_or_nothing(found)
        ),
    )
}

fn property_value(
    adapter: &Adapter<'_>,
    value: &AxValue,
    path: &str,
) -> Result<PropertyValue, AdaptError> {
    if let Some(related) = &value.related_nodes {
        return Ok(PropertyValue::Nodes(
            related
                .iter()
                .map(|related| adapter.related_node(related))
                .collect(),
        ));
    }
    match &value.value {
        None | Some(Value::Null) => Ok(PropertyValue::Undefined),
        Some(Value::Bool(boolean)) => Ok(PropertyValue::Boolean(*boolean)),
        Some(Value::Number(number)) => match number.as_i64() {
            Some(integer) if value.value_type != "number" => Ok(PropertyValue::Integer(integer)),
            _ => number.as_f64().map(PropertyValue::Number).ok_or_else(|| {
                AdaptError::field(
                    format!("{path}.value"),
                    format!("is {number}, not a number"),
                )
            }),
        },
        Some(Value::String(text)) => Ok(match value.value_type.as_str() {
            "token" | "tokenList" | "tristate" => PropertyValue::Token(text.clone()),
            _ => PropertyValue::Text(text.clone()),
        }),
        Some(other) => Err(AdaptError::field(
            format!("{path}.value"),
            format!(
                "expected a boolean, number, string or related nodes, found {}",
                excerpt(other)
            ),
        )),
    }
}

/// A scalar value as text: a string as it is, a number in its JSON decimal form, a boolean as
/// `true` or `false`. Absent for no value or `null`.
fn scalar_text(value: Option<&Value>, path: &str) -> Result<Option<String>, AdaptError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(Value::Number(number)) => Ok(Some(number.to_string())),
        Some(Value::Bool(boolean)) => Ok(Some(boolean.to_string())),
        Some(other) => Err(AdaptError::field(
            path,
            format!(
                "expected a string, number or boolean, found {}",
                excerpt(other)
            ),
        )),
    }
}

fn json_or_nothing(value: Option<&Value>) -> String {
    value.map_or_else(|| "nothing".to_owned(), excerpt)
}

fn exclusion_kind(reason: &str) -> ExclusionKind {
    match reason {
        "ariaHiddenElement" | "ariaHiddenSubtree" => ExclusionKind::AriaHidden,
        "inertElement" | "inertSubtree" => ExclusionKind::Inert,
        "notRendered" => ExclusionKind::NotRendered,
        "notVisible" => ExclusionKind::NotVisible,
        "presentationalRole" => ExclusionKind::PresentationalRole,
        "emptyAlt" => ExclusionKind::EmptyAlt,
        "labelFor" => ExclusionKind::LabelFor,
        "activeModalDialog" => ExclusionKind::OutsideActiveModal,
        "uninteresting" => ExclusionKind::Uninteresting,
        _ => ExclusionKind::Other,
    }
}

fn name_source_kind(kind: &str) -> NameSourceKind {
    match kind {
        "attribute" => NameSourceKind::Attribute,
        "implicit" => NameSourceKind::Implicit,
        "style" => NameSourceKind::Style,
        "contents" => NameSourceKind::Contents,
        "placeholder" => NameSourceKind::Placeholder,
        "relatedElement" => NameSourceKind::RelatedElement,
        other => NameSourceKind::Other(other.to_owned()),
    }
}

fn native_source(source: &str) -> NativeSource {
    match source {
        "label" => NativeSource::Label,
        "labelfor" => NativeSource::LabelFor,
        "labelwrapped" => NativeSource::LabelWrapped,
        "legend" => NativeSource::Legend,
        "figcaption" => NativeSource::Figcaption,
        "tablecaption" => NativeSource::TableCaption,
        "title" => NativeSource::Title,
        "rubyannotation" => NativeSource::RubyAnnotation,
        "description" => NativeSource::Description,
        other => NativeSource::Other(other.to_owned()),
    }
}

/// An author's `aria-posinset` or `aria-setsize`, when it parses as a positive integer.
fn positive_integer(value: &str) -> Option<u32> {
    value
        .trim_ascii()
        .parse::<u32>()
        .ok()
        .filter(|&value| value > 0)
}

/// A member count as a set position; a set too large for `u32` saturates.
fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
