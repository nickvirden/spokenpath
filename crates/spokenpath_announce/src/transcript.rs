//! The transcript types and the Layer A records built from the model.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Serialize;
use spokenpath_model::{
    AccessibleNode, Description, DescriptionSource, DomRef, Engine, ExcludedNode, ExclusionReason,
    Model, Name, NameSource, NameSourceKind, NativeSource, Property, PropertyValue, RelatedNode,
    Role, RoleOrigin, SetPosition, SourceClass, States, Supplied, SuppliedBy, TextPart,
    TextPartKind, TextRun, Tristate,
};

use crate::approx::Approx;
use crate::cross_check::{CrossCheck, ItemCrossCheck};
use crate::tab_order::TabOrder;

/// The ordered transcript of one model: what the browser exposes, item by item, with the
/// navigation indexes, the excluded nodes and the tab order.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Transcript {
    /// The engine and versions every Layer A record came from.
    pub header: Header,
    /// The parts of the transcript the engine does not supply; each one's field is left out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_available: Vec<NotAvailablePart>,
    /// The engine the roles and names were cross-checked against, and what it found; absent when
    /// no cross-check ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_check: Option<CrossCheck>,
    /// The exposed tree in reading order, one item per exposed node.
    pub nodes: Vec<Item>,
    /// The navigation indexes, each in reading order.
    pub indexes: Indexes,
    /// The DOM elements the engine leaves out of the exposed tree, in DOM order; absent when the
    /// engine does not supply them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excluded: Option<Vec<Excluded>>,
    /// The focus walk by repeated Tab presses and how it ended; absent when the engine does not
    /// supply it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_order: Option<TabOrder>,
    /// A second engine's transcript of the same page, with this transcript's items and its items
    /// aligned by source location; absent when no alignment ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aligned: Option<Aligned>,
}

/// A part of a transcript that an engine does not supply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NotAvailablePart {
    /// The sources tried for each name; every `NameRecord` leaves out `sources`.
    NameSources,
    /// The excluded nodes; the transcript leaves out `excluded`.
    Excluded,
    /// The focus walk; the transcript leaves out `tab_order`.
    TabOrder,
}

/// A second engine's transcript of the same page and how its items align with the first's.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Aligned {
    /// Safari's transcript of the same page.
    pub safari: Box<Transcript>,
    /// Every item of both transcripts, each in exactly one row, in row order.
    pub rows: Vec<AlignedRow>,
}

/// One row of an alignment: the items of each engine it holds, by number. A row that holds items
/// of both engines is aligned: every item in it is an accessible item located by the same span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AlignedRow {
    /// The Chromium items, by number, in reading order.
    pub chromium: Vec<usize>,
    /// The Safari items, by number, in reading order.
    pub safari: Vec<usize>,
}

/// The engine and versions a transcript came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Header {
    /// The engine token, such as `chromium`.
    pub engine: &'static str,
    /// The browser version, as the browser reports it.
    pub engine_version: String,
    /// The version of the tool that drove the browser.
    pub tool_version: String,
    /// Safari's build number, when the snapshot recorded it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safari_build: Option<String>,
    /// The macOS product version, when the snapshot recorded it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macos_version: Option<String>,
    /// The macOS build version, when the snapshot recorded it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macos_build: Option<String>,
}

/// One exposed node in reading order.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Item {
    /// The 1-based position in reading order.
    pub number: usize,
    /// The depth in the exposed tree; the root is 0.
    pub depth: usize,
    /// The number of the parent item; absent for the root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<usize>,
    /// What the browser exposes for the node.
    pub layer_a: LayerA,
    /// The node's role and name compared with the cross-check engine's; absent when no
    /// cross-check ran or the item is not compared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_check: Option<ItemCrossCheck>,
    /// The illustrative phrase, approximate; absent when Layer B is not produced.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approx: Option<Approx>,
}

/// The record of what the browser exposes for one item.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum LayerA {
    /// A node of the accessibility tree.
    Accessible(Box<AccessibleRecord>),
    /// A run of text.
    TextRun(TextRunRecord),
}

/// Whether a value is exposed by the engine or computed by the tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub enum Marking {
    /// The engine exposes the value.
    #[serde(rename = "exposed by engine")]
    ExposedByEngine,
    /// The tool derives the value from what the engine exposes.
    #[serde(rename = "computed by tool")]
    ComputedByTool,
}

impl From<SuppliedBy> for Marking {
    fn from(by: SuppliedBy) -> Self {
        match by {
            SuppliedBy::Engine => Marking::ExposedByEngine,
            SuppliedBy::Tool => Marking::ComputedByTool,
        }
    }
}

/// A value with its marking, for every value the model wraps in [`Supplied`] and for the
/// tab-order ending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[schemars(rename = "Marked_{T}")]
pub struct Marked<T> {
    /// The value.
    pub value: T,
    /// Who supplied it.
    pub by: Marking,
}

impl<T> Marked<T> {
    fn from_supplied<S>(supplied: &Supplied<S>, value: impl FnOnce(&S) -> T) -> Self {
        Marked {
            value: value(&supplied.value),
            by: supplied.by.into(),
        }
    }
}

/// Layer A of an accessible node.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct AccessibleRecord {
    /// The role the engine exposes; absent when it exposes none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<RoleRecord>,
    /// Where the role came from; absent when the node has no DOM element.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_origin: Option<Marked<RoleOriginToken>>,
    /// The DOM node, when the snapshot holds it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dom: Option<DomRecord>,
    /// Where in the page's sources the node came from.
    pub source: SourceRecord,
    /// The accessible name, or the plain statement that there is none, with every source tried.
    pub name: NameRecord,
    /// The accessible description, when the engine exposes one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<DescriptionRecord>,
    /// The value, as text, when the engine exposes one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// The typed states the engine exposes; absent ones are omitted.
    pub states: StatesRecord,
    /// Every other property, in engine order.
    pub properties: Vec<PropertyRecord>,
    /// The position in its set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_position: Option<Marked<SetPositionRecord>>,
}

/// A role token and whether it is an ARIA role, one internal to the engine, or one of the platform
/// accessibility API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RoleRecord {
    /// The role's text, as exposed.
    pub token: String,
    /// Whether the token is an ARIA role, internal to the engine, or the platform's.
    pub kind: RoleKind,
    /// A platform role's subrole text, when the platform gives one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subrole: Option<String>,
}

impl RoleRecord {
    /// The ARIA role token, when the role is one.
    #[must_use]
    pub fn aria(&self) -> Option<&str> {
        match self.kind {
            RoleKind::Aria => Some(&self.token),
            RoleKind::Internal | RoleKind::Platform => None,
        }
    }
}

/// Whether a role is an ARIA role, internal to the engine, or the platform's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RoleKind {
    /// An ARIA role token.
    Aria,
    /// A role the engine uses internally.
    Internal,
    /// A role of the platform accessibility API that stands for no single ARIA role.
    Platform,
}

/// Where an exposed role came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RoleOriginToken {
    /// The element's `role` attribute names the exposed role.
    Author,
    /// The element's own semantics.
    Native,
}

/// The identity of a DOM node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DomRecord {
    /// The snapshot-local DOM node id.
    pub dom_id: u32,
    /// The tag name of an element.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// The element's `id` attribute.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Where in the page's sources a node came from. The path is data only; nothing opens it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SourceRecord {
    /// The classification, from [`SourceClass::as_str`].
    pub class: &'static str,
    /// For a `span`, the source file's path relative to the capture root.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// For a `span`, the 1-based line of the start tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// For a `span`, the 1-based column of the start tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    /// For a `span`, the start tag's half-open range of UTF-8 byte offsets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_tag: Option<ByteRangeRecord>,
}

/// A half-open range of UTF-8 byte offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ByteRangeRecord {
    /// The first byte.
    pub start: u64,
    /// One past the last byte.
    pub end: u64,
}

/// Whether an item has an accessible name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub enum NameStatus {
    /// The name's text is not empty after trimming whitespace.
    #[serde(rename = "named")]
    Named,
    /// The name is absent, or empty after trimming whitespace.
    #[serde(rename = "no accessible name")]
    NoAccessibleName,
}

/// The text a [`NameStatus::NoAccessibleName`] serialises as.
pub const NO_ACCESSIBLE_NAME: &str = "no accessible name";

/// An accessible name and every source the engine tried for it.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct NameRecord {
    /// Whether there is a name.
    pub status: NameStatus,
    /// The name's text, raw, when the engine exposes a name (possibly empty or blank).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Every source the engine tried, in its order; empty when it reported none, and absent when
    /// the engine does not report name sources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<NameSourceRecord>>,
    /// Whether the winning source drew on content the engine hides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_hidden_content: Option<Marked<bool>>,
}

impl NameRecord {
    /// The name's text when the status is [`NameStatus::Named`].
    #[must_use]
    pub fn named_text(&self) -> Option<&str> {
        match self.status {
            NameStatus::Named => self.text.as_deref(),
            NameStatus::NoAccessibleName => None,
        }
    }
}

/// One source the engine tried for a name.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct NameSourceRecord {
    /// The kind of source, such as `attribute` or `contents`, or the engine's text for another.
    pub kind: String,
    /// The attribute the source reads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,
    /// The attribute's value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute_value: Option<String>,
    /// The native mechanism the source reads, such as `label-for`, or the engine's text for
    /// another.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_source: Option<String>,
    /// The text the source gave.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// The nodes the source drew on.
    pub related_nodes: Vec<RelatedRecord>,
    /// What came of the source.
    pub outcome: NameSourceOutcome,
    /// Why the engine marks the source invalid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid_reason: Option<String>,
}

/// What came of one name source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NameSourceOutcome {
    /// The source produced the name.
    Used,
    /// An earlier source took precedence.
    Superseded,
    /// The engine marks the source invalid.
    Invalid,
    /// The source gave nothing.
    Empty,
}

/// A node a value draws on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RelatedRecord {
    /// The DOM node, when the snapshot holds it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dom: Option<DomRecord>,
    /// The idref that named it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idref: Option<String>,
    /// The text it contributed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Where in the page's sources the node came from, when the model holds the node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceRecord>,
}

/// An accessible description and where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DescriptionRecord {
    /// The description's text.
    pub text: String,
    /// The attribute the description is attributed to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Marked<DescriptionSourceRecord>>,
}

/// The attribute a description is attributed to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "attribute", rename_all = "kebab-case")]
pub enum DescriptionSourceRecord {
    /// `aria-describedby`, with the elements its idrefs resolve to.
    AriaDescribedby {
        /// The elements.
        elements: Vec<RelatedRecord>,
    },
    /// `aria-description`.
    AriaDescription,
    /// `title`.
    Title,
}

/// The typed states an engine exposes; each absent one is omitted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct StatesRecord {
    /// Checked state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<TristateToken>,
    /// Pressed state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pressed: Option<TristateToken>,
    /// Expanded state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    /// Selected state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
    /// Disabled state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    /// Required state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// The invalid token, as the engine gives it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid: Option<String>,
    /// Read-only state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
    /// Whether the node can take focus.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focusable: Option<bool>,
    /// Whether the node had focus when the tree was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused: Option<bool>,
    /// The heading or tree level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
}

/// A three-valued state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TristateToken {
    /// On.
    True,
    /// Off.
    False,
    /// Partly on.
    Mixed,
}

/// A property the engine exposes that [`StatesRecord`] does not name.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct PropertyRecord {
    /// The engine's property name.
    pub name: String,
    /// Its typed value.
    pub value: PropertyValueRecord,
}

/// The typed value of a property.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum PropertyValueRecord {
    /// A boolean.
    Boolean(bool),
    /// An integer.
    Integer(i64),
    /// A number that may have a fraction.
    Number(f64),
    /// Free text.
    Text(String),
    /// A token from a fixed set.
    Token(String),
    /// A list of nodes.
    Nodes(Vec<RelatedRecord>),
    /// The engine names the property with no value.
    Undefined,
}

/// A node's position in its set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SetPositionRecord {
    /// The 1-based position.
    pub position: u32,
    /// The number of members; absent when unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u32>,
}

/// Layer A of a text run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TextRunRecord {
    /// The parts' text joined in order, with `"\n"` for each line break.
    pub text: String,
    /// The text nodes and line breaks the run came from, in order.
    pub parts: Vec<TextPartRecord>,
}

/// One text node or line break of a text run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TextPartRecord {
    /// Whether the part is text or a line break.
    pub kind: TextPartKindToken,
    /// The DOM node the part came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dom: Option<DomRecord>,
    /// Where in the page's sources the part came from.
    pub source: SourceRecord,
}

/// The kind of a text part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TextPartKindToken {
    /// A run of text.
    Text,
    /// A line break.
    LineBreak,
}

/// A DOM element the engine leaves out of the exposed tree.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Excluded {
    /// The element.
    pub dom: DomRecord,
    /// Where in the page's sources the element came from.
    pub source: SourceRecord,
    /// Every reason the engine gives, in its order.
    pub reasons: Vec<ExclusionRecord>,
    /// The role the element would have had, when the model holds one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<RoleRecord>,
    /// The name the element would have had, when the model holds one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<NameRecord>,
}

/// One reason an engine gives for leaving a node out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ExclusionRecord {
    /// The engine-neutral kind, from `ExclusionKind::as_str`.
    pub kind: &'static str,
    /// The engine's own name for the reason.
    pub engine_reason: String,
    /// The ancestor whose attribute or style hides the node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hiding_ancestor: Option<Marked<NodeRecord>>,
}

/// A DOM node with its source location, when the model holds the node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct NodeRecord {
    /// The DOM node.
    pub dom: DomRecord,
    /// Where in the page's sources the node came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceRecord>,
}

/// The navigation indexes, each in reading order and always present.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Indexes {
    /// Items whose ARIA role is `heading`.
    pub headings: Vec<HeadingEntry>,
    /// Items whose ARIA role is a landmark role.
    pub landmarks: Vec<IndexEntry>,
    /// Items whose ARIA role is `link`.
    pub links: Vec<IndexEntry>,
    /// Items whose ARIA role is a form-control role.
    pub form_controls: Vec<IndexEntry>,
}

/// An index entry: an item by number, its role and its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct IndexEntry {
    /// The item's number.
    pub item: usize,
    /// The item's ARIA role token.
    pub role: String,
    /// Whether the item has a name.
    pub name_status: NameStatus,
    /// The name's text, when the item has a name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A heading index entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct HeadingEntry {
    /// The item, role and name.
    #[serde(flatten)]
    pub entry: IndexEntry,
    /// The exposed level; absent when the engine exposes none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
}

/// The ARIA roles that list an item in [`Indexes::landmarks`].
pub const LANDMARK_ROLES: [&str; 8] = [
    "banner",
    "complementary",
    "contentinfo",
    "form",
    "main",
    "navigation",
    "region",
    "search",
];

/// The ARIA roles that list an item in [`Indexes::form_controls`].
pub const FORM_CONTROL_ROLES: [&str; 10] = [
    "button",
    "checkbox",
    "combobox",
    "listbox",
    "radio",
    "searchbox",
    "slider",
    "spinbutton",
    "switch",
    "textbox",
];

impl Indexes {
    /// Lists the item in every index its ARIA role qualifies it for.
    pub(crate) fn add(&mut self, number: usize, record: &AccessibleRecord) {
        let Some(role) = record.role.as_ref().and_then(RoleRecord::aria) else {
            return;
        };
        let entry = || IndexEntry {
            item: number,
            role: role.to_owned(),
            name_status: record.name.status,
            name: record.name.named_text().map(str::to_owned),
        };
        if role == "heading" {
            self.headings.push(HeadingEntry {
                entry: entry(),
                level: record.states.level,
            });
        } else if role == "link" {
            self.links.push(entry());
        } else if LANDMARK_ROLES.contains(&role) {
            self.landmarks.push(entry());
        } else if FORM_CONTROL_ROLES.contains(&role) {
            self.form_controls.push(entry());
        }
    }
}

/// The source location of every DOM node the model holds among its exposed and excluded nodes,
/// by DOM id; the first holder of an id wins.
pub(crate) struct DomSources<'m> {
    by_dom_id: BTreeMap<u32, &'m SourceClass>,
    /// Whether the engine reports name sources, so an absent name lists an empty set of them
    /// rather than none at all.
    name_sources_supplied: bool,
}

impl<'m> DomSources<'m> {
    pub(crate) fn new(model: &'m Model) -> Self {
        let mut by_dom_id = BTreeMap::new();
        for node in &model.nodes {
            if let Some(node) = node.as_accessible() {
                if let Some(dom) = &node.dom {
                    by_dom_id.entry(dom.dom_id).or_insert(&node.source);
                }
            } else if let Some(run) = node.as_text_run() {
                for part in &run.parts {
                    if let Some(dom) = &part.dom {
                        by_dom_id.entry(dom.dom_id).or_insert(&part.source);
                    }
                }
            }
        }
        for node in model.excluded.available().into_iter().flatten() {
            by_dom_id.entry(node.dom.dom_id).or_insert(&node.source);
        }
        DomSources {
            by_dom_id,
            name_sources_supplied: name_sources_supplied(model.engine),
        }
    }

    pub(crate) fn source(&self, dom: &DomRef) -> Option<SourceRecord> {
        self.by_dom_id
            .get(&dom.dom_id)
            .map(|source| source_record(source))
    }

    pub(crate) fn node(&self, dom: &DomRef) -> NodeRecord {
        NodeRecord {
            dom: dom_record(dom),
            source: self.source(dom),
        }
    }

    fn related(&self, node: &RelatedNode) -> RelatedRecord {
        RelatedRecord {
            dom: node.dom.as_ref().map(dom_record),
            idref: node.idref.clone(),
            text: node.text.clone(),
            source: node.dom.as_ref().and_then(|dom| self.source(dom)),
        }
    }

    fn related_list(&self, nodes: &[RelatedNode]) -> Vec<RelatedRecord> {
        nodes.iter().map(|node| self.related(node)).collect()
    }

    pub(crate) fn accessible(&self, node: &AccessibleNode) -> AccessibleRecord {
        AccessibleRecord {
            role: node.role.as_ref().map(role_record),
            role_origin: node.role_origin.as_ref().map(|origin| {
                Marked::from_supplied(origin, |origin| match origin {
                    RoleOrigin::Author => RoleOriginToken::Author,
                    RoleOrigin::Native => RoleOriginToken::Native,
                })
            }),
            dom: node.dom.as_ref().map(dom_record),
            source: source_record(&node.source),
            name: self.name(node.name.as_ref()),
            description: node
                .description
                .as_ref()
                .map(|description| self.description(description)),
            value: node.value.clone(),
            states: states_record(&node.states),
            properties: node
                .properties
                .iter()
                .map(|property| self.property(property))
                .collect(),
            set_position: node
                .set_position
                .as_ref()
                .map(|position| Marked::from_supplied(position, set_position_record)),
        }
    }

    pub(crate) fn name(&self, name: Option<&Name>) -> NameRecord {
        let Some(name) = name else {
            return NameRecord {
                status: NameStatus::NoAccessibleName,
                text: None,
                sources: self.name_sources_supplied.then(Vec::new),
                from_hidden_content: None,
            };
        };
        let status = if name.text.trim().is_empty() {
            NameStatus::NoAccessibleName
        } else {
            NameStatus::Named
        };
        NameRecord {
            status,
            text: Some(name.text.clone()),
            sources: name.sources.available().map(|sources| {
                sources
                    .iter()
                    .enumerate()
                    .map(|(index, source)| self.name_source(source, outcome(name, index, source)))
                    .collect()
            }),
            from_hidden_content: name
                .from_hidden_content
                .as_ref()
                .map(|hidden| Marked::from_supplied(hidden, |&hidden| hidden)),
        }
    }

    fn name_source(&self, source: &NameSource, outcome: NameSourceOutcome) -> NameSourceRecord {
        NameSourceRecord {
            kind: name_source_kind(&source.kind),
            attribute: source.attribute.clone(),
            attribute_value: source.attribute_value.clone(),
            native_source: source.native_source.as_ref().map(native_source),
            value: source.value.clone(),
            related_nodes: self.related_list(&source.related_nodes),
            outcome,
            invalid_reason: source.invalid_reason.clone(),
        }
    }

    fn description(&self, description: &Description) -> DescriptionRecord {
        DescriptionRecord {
            text: description.text.clone(),
            provenance: description.provenance.as_ref().map(|provenance| {
                Marked::from_supplied(provenance, |source| match source {
                    DescriptionSource::AriaDescribedby(elements) => {
                        DescriptionSourceRecord::AriaDescribedby {
                            elements: elements
                                .iter()
                                .map(|dom| RelatedRecord {
                                    dom: Some(dom_record(dom)),
                                    idref: None,
                                    text: None,
                                    source: self.source(dom),
                                })
                                .collect(),
                        }
                    }
                    DescriptionSource::AriaDescription => DescriptionSourceRecord::AriaDescription,
                    DescriptionSource::Title => DescriptionSourceRecord::Title,
                })
            }),
        }
    }

    fn property(&self, property: &Property) -> PropertyRecord {
        PropertyRecord {
            name: property.name.clone(),
            value: match &property.value {
                PropertyValue::Boolean(value) => PropertyValueRecord::Boolean(*value),
                PropertyValue::Integer(value) => PropertyValueRecord::Integer(*value),
                PropertyValue::Number(value) => PropertyValueRecord::Number(*value),
                PropertyValue::Text(value) => PropertyValueRecord::Text(value.clone()),
                PropertyValue::Token(value) => PropertyValueRecord::Token(value.clone()),
                PropertyValue::Nodes(nodes) => PropertyValueRecord::Nodes(self.related_list(nodes)),
                PropertyValue::Undefined => PropertyValueRecord::Undefined,
            },
        }
    }

    pub(crate) fn text_run(run: &TextRun) -> TextRunRecord {
        TextRunRecord {
            text: run.text.clone(),
            parts: run.parts.iter().map(text_part_record).collect(),
        }
    }

    pub(crate) fn excluded(&self, node: &ExcludedNode) -> Excluded {
        Excluded {
            dom: dom_record(&node.dom),
            source: source_record(&node.source),
            reasons: node
                .reasons
                .iter()
                .map(|reason| self.exclusion(reason))
                .collect(),
            role: node.role.as_ref().map(role_record),
            name: node.name.as_ref().map(|name| self.name(Some(name))),
        }
    }

    fn exclusion(&self, reason: &ExclusionReason) -> ExclusionRecord {
        ExclusionRecord {
            kind: reason.kind.as_str(),
            engine_reason: reason.engine_reason.clone(),
            hiding_ancestor: reason
                .hiding_ancestor
                .as_ref()
                .map(|ancestor| Marked::from_supplied(ancestor, |dom| self.node(dom))),
        }
    }
}

/// `used` for the winner, `invalid` when the engine marks the source invalid, `superseded` when an
/// earlier source took precedence (flagged by the engine, or after the winner), otherwise `empty`.
fn outcome(name: &Name, index: usize, source: &NameSource) -> NameSourceOutcome {
    match name.winner {
        Some(winner) if winner == index => NameSourceOutcome::Used,
        _ if source.invalid => NameSourceOutcome::Invalid,
        _ if source.superseded => NameSourceOutcome::Superseded,
        Some(winner) if index > winner => NameSourceOutcome::Superseded,
        _ => NameSourceOutcome::Empty,
    }
}

/// Whether an engine reports the sources it tried for a name. Chromium does; Safari's macOS
/// accessibility tree has no name provenance.
pub(crate) fn name_sources_supplied(engine: Engine) -> bool {
    match engine {
        Engine::Chromium => true,
        Engine::SafariAx | Engine::SafariWebdriver => false,
    }
}

fn role_record(role: &Role) -> RoleRecord {
    let (kind, subrole) = match role {
        Role::Aria(_) => (RoleKind::Aria, None),
        Role::Internal(_) => (RoleKind::Internal, None),
        Role::Platform { subrole, .. } => (RoleKind::Platform, subrole.clone()),
    };
    RoleRecord {
        token: role.as_str().to_owned(),
        kind,
        subrole,
    }
}

pub(crate) fn dom_record(dom: &DomRef) -> DomRecord {
    DomRecord {
        dom_id: dom.dom_id,
        tag: dom.tag.clone(),
        id: dom.id.clone(),
    }
}

fn source_record(source: &SourceClass) -> SourceRecord {
    let span = match source {
        SourceClass::Span(span) => Some(span),
        _ => None,
    };
    SourceRecord {
        class: source.as_str(),
        path: span.map(|span| span.path.clone()),
        line: span.map(|span| span.line),
        column: span.map(|span| span.column),
        start_tag: span.map(|span| ByteRangeRecord {
            start: span.start_tag.start,
            end: span.start_tag.end,
        }),
    }
}

fn name_source_kind(kind: &NameSourceKind) -> String {
    match kind {
        NameSourceKind::Attribute => "attribute",
        NameSourceKind::Implicit => "implicit",
        NameSourceKind::Style => "style",
        NameSourceKind::Contents => "contents",
        NameSourceKind::Placeholder => "placeholder",
        NameSourceKind::RelatedElement => "related-element",
        NameSourceKind::Other(text) => text,
    }
    .to_owned()
}

fn native_source(source: &NativeSource) -> String {
    match source {
        NativeSource::Label => "label",
        NativeSource::LabelFor => "label-for",
        NativeSource::LabelWrapped => "label-wrapped",
        NativeSource::Legend => "legend",
        NativeSource::Figcaption => "figcaption",
        NativeSource::TableCaption => "table-caption",
        NativeSource::Title => "title",
        NativeSource::RubyAnnotation => "ruby-annotation",
        NativeSource::Description => "description",
        NativeSource::Other(text) => text,
    }
    .to_owned()
}

fn tristate(value: Tristate) -> TristateToken {
    match value {
        Tristate::True => TristateToken::True,
        Tristate::False => TristateToken::False,
        Tristate::Mixed => TristateToken::Mixed,
    }
}

fn states_record(states: &States) -> StatesRecord {
    StatesRecord {
        checked: states.checked.map(tristate),
        pressed: states.pressed.map(tristate),
        expanded: states.expanded,
        selected: states.selected,
        disabled: states.disabled,
        required: states.required,
        invalid: states.invalid.clone(),
        readonly: states.readonly,
        focusable: states.focusable,
        focused: states.focused,
        level: states.level,
    }
}

fn set_position_record(position: &SetPosition) -> SetPositionRecord {
    SetPositionRecord {
        position: position.position,
        size: position.size,
    }
}

fn text_part_record(part: &TextPart) -> TextPartRecord {
    TextPartRecord {
        kind: match part.kind {
            TextPartKind::Text => TextPartKindToken::Text,
            TextPartKind::LineBreak => TextPartKindToken::LineBreak,
        },
        dom: part.dom.as_ref().map(dom_record),
        source: source_record(&part.source),
    }
}
