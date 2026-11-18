//! The model types. They name no engine's protocol structures; every engine's adapter fills them.

pub use spokenpath_snapshot::Engine;

/// What the browser exposes for one captured page, in engine-neutral form.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// The engine the snapshot came from.
    pub engine: Engine,
    /// The browser version, as the browser reports it.
    pub engine_version: String,
    /// The version of the tool that drove the browser.
    pub tool_version: String,
    /// The root of the exposed tree.
    pub root: NodeIndex,
    /// The exposed tree as an arena, in exposed-tree (pre-)order; [`NodeIndex`] values index it.
    pub nodes: Vec<ExposedNode>,
    /// The DOM elements the engine leaves out of the exposed tree, in DOM order.
    pub excluded: Vec<ExcludedNode>,
    /// The walk of focus by repeated Tab presses, as captured.
    pub focus: FocusWalk,
    /// The problems the capture recorded.
    pub diagnostics: Vec<Diagnostic>,
}

impl Model {
    /// The exposed node at `index`, or `None` when the index belongs to another model.
    #[must_use]
    pub fn node(&self, index: NodeIndex) -> Option<&ExposedNode> {
        self.nodes.get(index.0)
    }

    /// The exposed nodes with their indexes, in exposed-tree order.
    pub fn indexed_nodes(&self) -> impl Iterator<Item = (NodeIndex, &ExposedNode)> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (NodeIndex(index), node))
    }
}

/// The position of an exposed node in [`Model::nodes`]. Only an adapter creates one, so an index
/// from a model always resolves in that model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeIndex(pub(crate) usize);

impl NodeIndex {
    /// The position in [`Model::nodes`].
    #[must_use]
    pub fn get(self) -> usize {
        self.0
    }
}

/// Whether a value was supplied by the engine or computed by the tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppliedBy {
    /// The engine exposes the value.
    Engine,
    /// The tool derives the value from what the engine exposes; output labels it "computed by
    /// tool".
    Tool,
}

/// A value together with who supplied it, for every value that the tool may derive rather than
/// read.
#[derive(Debug, Clone, PartialEq)]
pub struct Supplied<T> {
    /// The value.
    pub value: T,
    /// Who supplied it.
    pub by: SuppliedBy,
}

impl<T> Supplied<T> {
    /// A value the engine exposes.
    pub fn engine(value: T) -> Self {
        Supplied {
            value,
            by: SuppliedBy::Engine,
        }
    }

    /// A value computed by the tool.
    pub fn tool(value: T) -> Self {
        Supplied {
            value,
            by: SuppliedBy::Tool,
        }
    }
}

/// One node of the exposed tree.
#[derive(Debug, Clone, PartialEq)]
pub enum ExposedNode {
    /// A node of the accessibility tree.
    Accessible(Box<AccessibleNode>),
    /// Adjacent text nodes merged into one run of text.
    TextRun(TextRun),
}

impl ExposedNode {
    /// The parent in the exposed tree; absent only for the root.
    #[must_use]
    pub fn parent(&self) -> Option<NodeIndex> {
        match self {
            ExposedNode::Accessible(node) => node.parent,
            ExposedNode::TextRun(run) => Some(run.parent),
        }
    }

    /// The children in the exposed tree; a text run has none.
    #[must_use]
    pub fn children(&self) -> &[NodeIndex] {
        match self {
            ExposedNode::Accessible(node) => &node.children,
            ExposedNode::TextRun(_) => &[],
        }
    }

    /// The node, when it is an accessible node rather than a text run.
    #[must_use]
    pub fn as_accessible(&self) -> Option<&AccessibleNode> {
        match self {
            ExposedNode::Accessible(node) => Some(node),
            ExposedNode::TextRun(_) => None,
        }
    }

    /// The run, when the node is a text run.
    #[must_use]
    pub fn as_text_run(&self) -> Option<&TextRun> {
        match self {
            ExposedNode::Accessible(_) => None,
            ExposedNode::TextRun(run) => Some(run),
        }
    }
}

/// An exposed node of the accessibility tree.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessibleNode {
    /// The parent in the exposed tree; absent only for the root.
    pub parent: Option<NodeIndex>,
    /// The exposed children, in the engine's child order.
    pub children: Vec<NodeIndex>,
    /// The role the engine exposes; absent when it exposes none.
    pub role: Option<Role>,
    /// Where the role came from, computed by tool; absent when the node has no DOM element.
    pub role_origin: Option<Supplied<RoleOrigin>>,
    /// The DOM node the accessibility node belongs to, when the snapshot holds it.
    pub dom: Option<DomRef>,
    /// Where in the page's sources the node came from.
    pub source: SourceClass,
    /// The accessible name, when the engine exposes one.
    pub name: Option<Name>,
    /// The accessible description, when the engine exposes one.
    pub description: Option<Description>,
    /// The value, as text, when the engine exposes one.
    pub value: Option<String>,
    /// The common states the engine exposes.
    pub states: States,
    /// Every other property the engine exposes, in its order.
    pub properties: Vec<Property>,
    /// The position in its set, computed by tool, for `listitem`, `option` and `radio` nodes that
    /// belong to a set.
    pub set_position: Option<Supplied<SetPosition>>,
}

/// A role: an ARIA role token, or a role internal to the engine kept as its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    /// An ARIA role token, such as `button`.
    Aria(String),
    /// A role the engine uses internally, such as Chromium's `RootWebArea` or `LabelText`.
    Internal(String),
}

impl Role {
    /// The ARIA role token, when the role is one.
    #[must_use]
    pub fn aria(&self) -> Option<&str> {
        match self {
            Role::Aria(role) => Some(role),
            Role::Internal(_) => None,
        }
    }

    /// The role's text, ARIA or internal.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Role::Aria(role) | Role::Internal(role) => role,
        }
    }
}

/// Where an exposed role came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleOrigin {
    /// The element's `role` attribute names the exposed role.
    Author,
    /// The element's own semantics: its `role` attribute, if any, does not name the exposed role.
    Native,
}

/// The identity of a DOM node in the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomRef {
    /// The snapshot-local DOM node id.
    pub dom_id: u32,
    /// The tag name of an element, as the browser reports it.
    pub tag: Option<String>,
    /// The element's `id` attribute, when it has one.
    pub id: Option<String>,
}

/// Where in the page's sources a node came from. It is never blank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceClass {
    /// An element written in a source file, located by its span.
    Span(SpanRef),
    /// An element the HTML parser inserted, such as an implied `tbody`.
    Implied,
    /// An element a page script created.
    Script,
    /// An element a known library created.
    Library,
    /// An element whose origin could not be established, such as user-agent shadow content.
    Unknown,
    /// The document itself.
    Document,
    /// A text node.
    Text,
    /// A node the engine references but the snapshot's DOM does not hold, such as
    /// pseudo-element content.
    NotInDom,
}

impl SourceClass {
    /// The classification's name: `span`, `implied`, `script`, `library`, `unknown`, `document`,
    /// `text` or `not-in-dom`.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceClass::Span(_) => "span",
            SourceClass::Implied => "implied",
            SourceClass::Script => "script",
            SourceClass::Library => "library",
            SourceClass::Unknown => "unknown",
            SourceClass::Document => "document",
            SourceClass::Text => "text",
            SourceClass::NotInDom => "not-in-dom",
        }
    }
}

/// The location of an element's start tag in a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanRef {
    /// The snapshot-local span id.
    pub span: u32,
    /// The snapshot-local source id.
    pub source: u32,
    /// The source file's path relative to the capture root.
    pub path: String,
    /// The 1-based line of the start tag's `<`.
    pub line: u32,
    /// The 1-based column of the start tag's `<`, in Unicode scalar values.
    pub column: u32,
    /// The start tag's half-open range of UTF-8 byte offsets.
    pub start_tag: ByteRange,
}

/// A half-open range of UTF-8 byte offsets into a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    /// The first byte.
    pub start: u64,
    /// One past the last byte.
    pub end: u64,
}

/// Adjacent text nodes of one child list, merged into one run of text.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    /// The exposed node whose child list holds the run.
    pub parent: NodeIndex,
    /// The parts' text joined in order, with `"\n"` for each line break.
    pub text: String,
    /// The text nodes and line breaks the run came from, in order.
    pub parts: Vec<TextPart>,
}

/// One text node or line break of a [`TextRun`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextPart {
    /// Whether the part is text or a line break.
    pub kind: TextPartKind,
    /// The DOM node the part came from, when the snapshot holds it.
    pub dom: Option<DomRef>,
    /// Where in the page's sources the part came from.
    pub source: SourceClass,
}

/// The kind of a [`TextPart`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextPartKind {
    /// A run of text.
    Text,
    /// A line break, which contributes `"\n"` to the run's text.
    LineBreak,
}

/// An accessible name and how the engine derived it.
#[derive(Debug, Clone, PartialEq)]
pub struct Name {
    /// The name's text, possibly empty.
    pub text: String,
    /// Every source the engine tried, in the order it tried them.
    pub sources: Vec<NameSource>,
    /// The index in `sources` of the source that produced the name: the first with a value that is
    /// not superseded. Absent when no source produced one.
    pub winner: Option<usize>,
    /// Whether the winning source's related nodes include an excluded node, computed by tool.
    /// Absent when there is no winning source or it has no related nodes.
    pub from_hidden_content: Option<Supplied<bool>>,
}

impl Name {
    /// The source that produced the name.
    #[must_use]
    pub fn winning_source(&self) -> Option<&NameSource> {
        self.winner.and_then(|index| self.sources.get(index))
    }
}

/// One source the engine tried for a name.
#[derive(Debug, Clone, PartialEq)]
pub struct NameSource {
    /// What kind of source it is.
    pub kind: NameSourceKind,
    /// The attribute the source reads, such as `aria-label`.
    pub attribute: Option<String>,
    /// The attribute's value, as text.
    pub attribute_value: Option<String>,
    /// The native mechanism the source reads, such as a `label` element.
    pub native_source: Option<NativeSource>,
    /// The text this source gave, when it gave any.
    pub value: Option<String>,
    /// The nodes the source drew on, from its value, attribute value and native source value.
    pub related_nodes: Vec<RelatedNode>,
    /// True when an earlier source took precedence over this one.
    pub superseded: bool,
    /// True when the engine marks the source invalid.
    pub invalid: bool,
    /// Why the engine marks the source invalid.
    pub invalid_reason: Option<String>,
}

/// The kind of a [`NameSource`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameSourceKind {
    /// An attribute of the element, such as `aria-label`, `title` or `alt`.
    Attribute,
    /// A value the engine supplies implicitly.
    Implicit,
    /// A value from styling.
    Style,
    /// The element's contents.
    Contents,
    /// A placeholder attribute.
    Placeholder,
    /// Another element, such as an `aria-labelledby` target or a `label`.
    RelatedElement,
    /// A kind this model does not name, kept as the engine's text.
    Other(String),
}

/// The native mechanism a [`NameSource`] reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeSource {
    /// A `label` element.
    Label,
    /// A `label` element whose `for` names the element.
    LabelFor,
    /// A `label` element wrapping the element.
    LabelWrapped,
    /// A `fieldset`'s `legend`.
    Legend,
    /// A `figure`'s `figcaption`.
    Figcaption,
    /// A `table`'s `caption`.
    TableCaption,
    /// The `title` element or attribute.
    Title,
    /// A ruby annotation.
    RubyAnnotation,
    /// A description mechanism.
    Description,
    /// A mechanism this model does not name, kept as the engine's text.
    Other(String),
}

/// A node a value draws on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedNode {
    /// The DOM node, when the snapshot holds it.
    pub dom: Option<DomRef>,
    /// The idref that named it.
    pub idref: Option<String>,
    /// The text it contributed.
    pub text: Option<String>,
}

/// An accessible description and where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Description {
    /// The description's text.
    pub text: String,
    /// The attribute the description is attributed to, computed by tool; absent when no attribute
    /// accounts for it.
    pub provenance: Option<Supplied<DescriptionSource>>,
}

/// The attribute a description is attributed to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescriptionSource {
    /// `aria-describedby`, with the elements its idrefs resolve to.
    AriaDescribedby(Vec<DomRef>),
    /// `aria-description`.
    AriaDescription,
    /// `title`.
    Title,
}

/// The common states an engine exposes; each is absent when the engine does not expose it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct States {
    /// Checked state.
    pub checked: Option<Tristate>,
    /// Pressed state.
    pub pressed: Option<Tristate>,
    /// Expanded state.
    pub expanded: Option<bool>,
    /// Selected state.
    pub selected: Option<bool>,
    /// Disabled state.
    pub disabled: Option<bool>,
    /// Required state.
    pub required: Option<bool>,
    /// The invalid token, such as `"false"`, `"true"` or `"spelling"`, kept as the engine gives it.
    pub invalid: Option<String>,
    /// Read-only state.
    pub readonly: Option<bool>,
    /// Whether the node can take focus.
    pub focusable: Option<bool>,
    /// Whether the node had focus when the tree was read.
    pub focused: Option<bool>,
    /// The heading or tree level.
    pub level: Option<u32>,
}

/// A three-valued state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tristate {
    /// On.
    True,
    /// Off.
    False,
    /// Partly on.
    Mixed,
}

/// A property the engine exposes that [`States`] does not name.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    /// The engine's property name.
    pub name: String,
    /// Its value.
    pub value: PropertyValue,
}

/// The typed value of a [`Property`].
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    /// A boolean.
    Boolean(bool),
    /// An integer.
    Integer(i64),
    /// A number that may have a fraction.
    Number(f64),
    /// Free text.
    Text(String),
    /// A token from a fixed set, such as `vertical`.
    Token(String),
    /// A list of nodes.
    Nodes(Vec<RelatedNode>),
    /// The engine names the property with no value.
    Undefined,
}

/// A node's position in its set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetPosition {
    /// The 1-based position.
    pub position: u32,
    /// The number of members; absent when the author marks it unknown with `aria-setsize="-1"`.
    pub size: Option<u32>,
}

/// A DOM element the engine leaves out of the exposed tree.
#[derive(Debug, Clone, PartialEq)]
pub struct ExcludedNode {
    /// The element.
    pub dom: DomRef,
    /// Where in the page's sources the element came from.
    pub source: SourceClass,
    /// Why the engine leaves it out, in the engine's order; empty when the engine gives none.
    pub reasons: Vec<ExclusionReason>,
    /// The role the element would have had, when the snapshot records one. The Chromium adapter
    /// always leaves it absent, because Chromium does not expose it for ignored nodes.
    pub role: Option<Role>,
    /// The name the element would have had, when the snapshot records one. The Chromium adapter
    /// always leaves it absent, because Chromium does not expose it for ignored nodes.
    pub name: Option<Name>,
}

/// One reason an engine gives for leaving a node out.
#[derive(Debug, Clone, PartialEq)]
pub struct ExclusionReason {
    /// The engine-neutral kind.
    pub kind: ExclusionKind,
    /// The engine's own name for the reason, such as `ariaHiddenSubtree`.
    pub engine_reason: String,
    /// The ancestor whose attribute or style hides the node; absent when the node hides itself or
    /// the data to find it is missing.
    pub hiding_ancestor: Option<Supplied<DomRef>>,
}

/// The engine-neutral kind of an [`ExclusionReason`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusionKind {
    /// `aria-hidden` on the node or an ancestor.
    AriaHidden,
    /// `inert` on the node or an ancestor.
    Inert,
    /// The node is not rendered, as with `display: none` or `hidden`.
    NotRendered,
    /// The node is rendered but not visible, as with `visibility: hidden`.
    NotVisible,
    /// A presentational role removes the node's semantics.
    PresentationalRole,
    /// An image with an empty `alt`.
    EmptyAlt,
    /// A `label` the engine folds into the control it labels.
    LabelFor,
    /// The node is outside an active modal dialog.
    OutsideActiveModal,
    /// The engine finds nothing in the node worth exposing.
    Uninteresting,
    /// A reason this model does not name; the engine's text is in
    /// [`ExclusionReason::engine_reason`].
    Other,
}

impl ExclusionKind {
    /// The kind's name, such as `aria-hidden` or `not-rendered`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ExclusionKind::AriaHidden => "aria-hidden",
            ExclusionKind::Inert => "inert",
            ExclusionKind::NotRendered => "not-rendered",
            ExclusionKind::NotVisible => "not-visible",
            ExclusionKind::PresentationalRole => "presentational-role",
            ExclusionKind::EmptyAlt => "empty-alt",
            ExclusionKind::LabelFor => "label-for",
            ExclusionKind::OutsideActiveModal => "outside-active-modal",
            ExclusionKind::Uninteresting => "uninteresting",
            ExclusionKind::Other => "other",
        }
    }
}

/// The walk of focus by repeated Tab presses, as captured; the model does not interpret traps or
/// cycles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusWalk {
    /// One stop per Tab press, in order.
    pub steps: Vec<FocusStop>,
    /// True when the walk stopped at its step limit.
    pub truncated: bool,
    /// The index in `steps` of the first step whose element an earlier step already focused.
    pub cycle_start: Option<u32>,
}

/// Where focus was after one Tab press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FocusStop {
    /// Focus left the page.
    LeftPage,
    /// An element of the snapshot's DOM.
    Element {
        /// The focused element.
        dom: DomRef,
        /// The exposed node of that element, when it is exposed.
        node: Option<NodeIndex>,
    },
    /// An element the snapshot's DOM does not hold, known only by the engine's node id.
    Unresolved {
        /// The engine's id for the node.
        backend_id: u64,
    },
}

/// A problem the capture recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// A stable, machine-readable code.
    pub code: String,
    /// The capture's message.
    pub message: String,
    /// The DOM node the problem concerns.
    pub dom: Option<DomRef>,
}
