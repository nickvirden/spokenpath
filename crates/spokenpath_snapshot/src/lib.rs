//! The versioned JSON snapshot that a capture layer writes and the Rust core reads, which is the contract between capture and core.
//!
//! A snapshot records what the browser exposes, raw and uninterpreted: a common envelope plus an
//! engine-specific payload. [`read`] accepts any snapshot whose major version is
//! [`SUPPORTED_MAJOR`], whatever its minor version, and ignores fields it does not know, except in
//! the accessibility nodes, where unknown protocol fields are kept and written back.
//! [`json_schema`] is the JSON Schema that `cargo xtask schema` writes to
//! `schema/snapshot.v0.json`.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

/// The value of every snapshot's `schema` field.
pub const SCHEMA_ID: &str = "spokenpath-snapshot";

/// The one major version this reader reads.
pub const SUPPORTED_MAJOR: u32 = 0;

/// The newest version this release defines. Each writer writes the lowest minor version that
/// defines every field it writes, so a writer whose payload uses no field added since minor 0 keeps
/// writing minor 0.
pub const CURRENT_VERSION: Version = Version {
    major: SUPPORTED_MAJOR,
    minor: 1,
};

/// One capture of one page in one engine: the envelope that every engine shares, plus the
/// engine-specific payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Snapshot {
    /// Identifies the file as a spokenpath snapshot.
    pub schema: SchemaId,
    pub version: Version,
    pub producer: Producer,
    pub capture: Capture,
    pub volatile: Volatile,
    /// The source files that spans point into.
    pub sources: Vec<Source>,
    /// Locations of tagged start tags in the source files.
    pub spans: Vec<Span>,
    /// Problems the capture noticed and recorded rather than failing on.
    pub diagnostics: Vec<Diagnostic>,
    pub payload: Payload,
}

/// The fixed identifier `spokenpath-snapshot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchemaId {
    #[serde(rename = "spokenpath-snapshot")]
    SpokenpathSnapshot,
}

/// Written by hand so the schema states the identifier as a `const`, which validators report as
/// a wrong value rather than as a failed enumeration.
impl JsonSchema for SchemaId {
    fn schema_name() -> Cow<'static, str> {
        "SchemaId".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "The fixed identifier `spokenpath-snapshot`.",
            "type": "string",
            "const": SCHEMA_ID,
        })
    }
}

/// The format version. A reader rejects a major version it does not know and accepts any minor
/// version, because minor versions only add fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Version {
    #[schemars(extend("const" = SUPPORTED_MAJOR))]
    pub major: u32,
    pub minor: u32,
}

/// The program that wrote the snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Producer {
    pub name: String,
    pub version: String,
}

/// The engine and the conditions the page was captured under.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Capture {
    /// The engine the snapshot came from; it equals `payload.engine`.
    pub engine: Engine,
    /// The browser version, as the browser reports it.
    pub engine_version: String,
    /// The version of the tool that drove the browser, for example Playwright.
    pub tool_version: String,
    pub viewport: Viewport,
    pub color_scheme: ColorScheme,
    pub reduced_motion: ReducedMotion,
    /// The URL the page was loaded from.
    pub url: String,
    /// The name of the strategy that decided the page had settled; the capture backends define
    /// the values.
    pub settle: String,
}

/// The engine a snapshot came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Engine {
    Chromium,
    SafariWebdriver,
    SafariAx,
}

impl Engine {
    /// The name the engine has in a snapshot.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Engine::Chromium => "chromium",
            Engine::SafariWebdriver => "safari-webdriver",
            Engine::SafariAx => "safari-ax",
        }
    }
}

impl fmt::Display for Engine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The viewport, in CSS pixels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    pub device_pixel_ratio: f64,
}

/// The emulated `prefers-color-scheme`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ColorScheme {
    Light,
    Dark,
    NoPreference,
}

/// The emulated `prefers-reduced-motion`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ReducedMotion {
    Reduce,
    NoPreference,
}

/// Fields that differ on every run of the same capture. Golden comparisons exclude this whole
/// object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Volatile {
    /// When the capture ran, as an RFC 3339 timestamp in UTC.
    #[schemars(regex(
        pattern = r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?Z$"
    ))]
    pub captured_at: String,
}

/// A source file the page was built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Source {
    /// Snapshot-local id that spans refer to.
    pub id: u32,
    /// The file's path relative to the capture root: `/`-separated segments, none of them empty,
    /// `.` or `..`, with no leading `/` and no `\`, `:` or NUL, so joining it onto the root never
    /// leaves the root.
    #[schemars(regex(pattern = SOURCE_PATH_PATTERN))]
    pub path: String,
    /// SHA-256 of the file's bytes, as 64 lowercase hex characters.
    #[schemars(regex(pattern = r"^[0-9a-f]{64}$"))]
    pub sha256: String,
    pub encoding: Encoding,
}

/// One segment of a source path: not empty, not `.` or `..`, and free of `/`, `\`, `:` and NUL.
/// It is written without lookahead so ECMAScript and Rust regex engines read it the same way.
macro_rules! source_path_segment {
    () => {
        r"(?:[^/\\:\x00.][^/\\:\x00]*|\.[^/\\:\x00.][^/\\:\x00]*|\.\.[^/\\:\x00]+)"
    };
}

/// The pattern the schema states for [`Source::path`]; [`is_source_path`] is the reader's check of
/// the same rule.
const SOURCE_PATH_PATTERN: &str = concat!(
    "^",
    source_path_segment!(),
    "(?:/",
    source_path_segment!(),
    ")*$"
);

/// Whether `path` is a capture-root-relative source path, as [`SOURCE_PATH_PATTERN`] states.
fn is_source_path(path: &str) -> bool {
    !path.is_empty()
        && path.split('/').all(|segment| {
            !matches!(segment, "" | "." | "..") && !segment.contains(['\\', ':', '\0'])
        })
}

/// The encoding of a source file; only UTF-8 is supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Encoding {
    #[serde(rename = "utf-8")]
    Utf8,
}

/// The location of one tagged start tag in a source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Span {
    /// Snapshot-local id that DOM nodes refer to.
    pub id: u32,
    /// The `id` of the source file the ranges point into.
    pub source: u32,
    /// The start tag, from its `<` to just past its `>`.
    pub start_tag: ByteRange,
    /// The whole element, from its start tag to the end of its end tag, when the element has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element: Option<ByteRange>,
    /// The 1-based line of the start tag's `<`.
    #[schemars(range(min = 1))]
    pub line: u32,
    /// The 1-based column of the start tag's `<`, counted in Unicode scalar values from the start
    /// of the line.
    #[schemars(range(min = 1))]
    pub column: u32,
}

/// A half-open range of UTF-8 byte offsets into a source file: `start` is the first byte and `end`
/// is one past the last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ByteRange {
    /// UTF-8 byte offset of the first byte, from the start of the file.
    pub start: u64,
    /// UTF-8 byte offset one past the last byte, from the start of the file.
    pub end: u64,
}

/// A problem the capture recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Diagnostic {
    /// A stable, machine-readable code.
    pub code: String,
    pub message: String,
    /// The `id` of the DOM node the problem concerns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<u32>,
}

/// The engine-specific part of the snapshot, tagged by `engine`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "engine", rename_all = "kebab-case")]
pub enum Payload {
    Chromium(ChromiumPayload),
    SafariWebdriver(SafariWebdriverPayload),
    SafariAx(SafariAxPayload),
}

impl Payload {
    /// The engine this payload belongs to.
    #[must_use]
    pub fn engine(&self) -> Engine {
        match self {
            Payload::Chromium(_) => Engine::Chromium,
            Payload::SafariWebdriver(_) => Engine::SafariWebdriver,
            Payload::SafariAx(_) => Engine::SafariAx,
        }
    }
}

/// What Safari answers over `WebDriver` for the page's tagged elements: the computed role and the
/// computed label of each, with the Safari and macOS builds that answered. Minor version 1 adds
/// every field; a minor-0 payload has none and reads with no elements and no versions.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct SafariWebdriverPayload {
    /// One entry per element in Safari's DOM whose `data-spokenpath-src` value names a span, in
    /// document order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<SafariElement>,
    /// Safari's `CFBundleVersion`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safari_build: Option<String>,
    /// The macOS product version, from `sw_vers`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos_version: Option<String>,
    /// The macOS build version, from `sw_vers`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos_build: Option<String>,
}

/// Safari's answers for one tagged element. Exactly one of `role` and `role_error` is present, and
/// exactly one of `label` and `label_error`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SafariElement {
    /// The `id` of the span the element's `data-spokenpath-src` names.
    pub span: u32,
    /// The Get Computed Role answer, possibly empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// The error Get Computed Role answered with instead of a role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_error: Option<WebdriverError>,
    /// The Get Computed Label answer, possibly empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The error Get Computed Label answered with instead of a label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_error: Option<WebdriverError>,
}

/// One answer to a `WebDriver` call: the value, or the error the browser answered with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebdriverAnswer<'a> {
    /// The value the call returned.
    Value(&'a str),
    /// The error the call returned instead.
    Error(&'a WebdriverError),
}

impl SafariElement {
    /// The Get Computed Role answer, or `None` when the entry holds both or neither of `role` and
    /// `role_error`, which [`read`] rejects.
    #[must_use]
    pub fn role_answer(&self) -> Option<WebdriverAnswer<'_>> {
        answer(self.role.as_deref(), self.role_error.as_ref())
    }

    /// The Get Computed Label answer, or `None` when the entry holds both or neither of `label`
    /// and `label_error`, which [`read`] rejects.
    #[must_use]
    pub fn label_answer(&self) -> Option<WebdriverAnswer<'_>> {
        answer(self.label.as_deref(), self.label_error.as_ref())
    }
}

fn answer<'a>(
    value: Option<&'a str>,
    error: Option<&'a WebdriverError>,
) -> Option<WebdriverAnswer<'a>> {
    match (value, error) {
        (Some(value), None) => Some(WebdriverAnswer::Value(value)),
        (None, Some(error)) => Some(WebdriverAnswer::Error(error)),
        _ => None,
    }
}

/// A `WebDriver` error response, as the browser returned it, without its stack trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WebdriverError {
    /// The `WebDriver` error code, such as `unknown error`.
    pub error: String,
    /// The message, possibly empty.
    pub message: String,
}

/// The Safari payload captured from the macOS accessibility tree. Its contents are added in later
/// minor versions.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct SafariAxPayload {}

/// What Chromium exposes for the page, as the `DevTools` protocol delivers it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChromiumPayload {
    /// Every DOM node, flat, with the document first.
    pub dom: Vec<DomNode>,
    /// The nodes of `Accessibility.getFullAXTree`.
    pub ax: Vec<AxNode>,
    /// One `Accessibility.getPartialAXTree` result per DOM element that has no node in the full
    /// tree, kept apart from `ax`.
    pub ax_forced: Vec<AxForced>,
    /// The raw `DOMSnapshot.captureSnapshot` response, with computed `display` and `visibility`,
    /// kept as delivered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dom_snapshot: Option<Map<String, Value>>,
    pub focus: Focus,
}

/// One DOM node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DomNode {
    /// Snapshot-local id.
    pub id: u32,
    /// The protocol's `backendNodeId`.
    pub backend_id: u64,
    /// The parent's `id`; absent only for the root. A shadow root's parent is its host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u32>,
    /// The children's `id`s, in DOM order. A shadow host lists its shadow root first.
    pub children: Vec<u32>,
    pub kind: DomNodeKind,
    /// The namespace URI of an element.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// The tag name of an element, as the browser reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    /// The attributes in their DOM order, with `data-spokenpath-src` removed.
    pub attributes: Vec<Attribute>,
    /// The text of a text-bearing node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<Text>,
    /// The mode of a shadow root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<ShadowMode>,
    /// The `id` of the span locating the element's start tag in the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<u32>,
    /// Where an element came from; set on element nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
}

/// The type of a DOM node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DomNodeKind {
    Document,
    Doctype,
    Element,
    Text,
    Comment,
    CdataSection,
    ProcessingInstruction,
    DocumentFragment,
}

/// One attribute of an element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Attribute {
    pub name: String,
    pub value: String,
}

/// The text of a text-bearing node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Text {
    pub value: String,
    /// True when the capture shortened `value`.
    pub truncated: bool,
}

/// The mode of a shadow root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ShadowMode {
    Open,
    Closed,
    UserAgent,
}

/// Where an element came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    /// Written in a source file, with a span.
    Source,
    /// Inserted by the HTML parser, such as an implied `tbody`.
    Implied,
    /// Created by a page script.
    Script,
    /// Created by a known library.
    Library,
    /// None of the above could be established.
    Unknown,
}

/// The partial-tree result for one DOM element missing from the full tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AxForced {
    /// The DOM node's `id`.
    pub node: u32,
    /// The DOM node's `backendNodeId`, which the query used.
    pub backend_id: u64,
    /// The response's node list, as delivered.
    pub nodes: Vec<AxNode>,
}

/// An accessibility node: a typed subset of the protocol's `AXNode`, in the protocol's names.
/// Name provenance (`name.sources`) and exclusion reasons (`ignoredReasons`) are optional because
/// not every engine provides them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AxNode {
    pub node_id: String,
    pub ignored: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ignored_reasons: Option<Vec<AxProperty>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<AxValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<AxValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<AxValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<AxValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<Vec<AxProperty>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child_ids: Option<Vec<String>>,
    #[serde(
        rename = "backendDOMNodeId",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub backend_dom_node_id: Option<u64>,
    /// Protocol fields these types do not name, kept as delivered and written back.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// A named value: an `AXProperty`, or an entry of `ignoredReasons`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AxProperty {
    pub name: String,
    pub value: AxValue,
    /// Protocol fields these types do not name, kept as delivered and written back.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The protocol's `AXValue`. `type` stays a string so a value type the protocol adds later is kept
/// rather than rejected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AxValue {
    #[serde(rename = "type")]
    pub value_type: String,
    // Any JSON value; an explicit `null` is kept as `null`. The field carries no doc comment
    // because the TypeScript generator reads a schema holding only a description as an object.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_nodes: Option<Vec<AxRelatedNode>>,
    /// Where the value came from, in the order the engine tried them (name provenance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<AxValueSource>>,
    /// Protocol fields these types do not name, kept as delivered and written back.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The protocol's `AXRelatedNode`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AxRelatedNode {
    #[serde(rename = "backendDOMNodeId")]
    pub backend_dom_node_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Protocol fields these types do not name, kept as delivered and written back.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The protocol's `AXValueSource`: one source the engine tried for a value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AxValueSource {
    #[serde(rename = "type")]
    pub source_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<AxValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribute_value: Option<AxValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_source_value: Option<AxValue>,
    /// Protocol fields these types do not name, kept as delivered and written back.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// The walk of focus by repeated Tab presses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Focus {
    /// One step per Tab press, in order.
    pub steps: Vec<FocusStep>,
    /// True when the walk stopped at its step limit.
    pub truncated: bool,
    /// The index in `steps` of the first step whose element an earlier step already focused.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_start: Option<u32>,
}

/// The element focused after one Tab press; both fields are absent when focus left the document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FocusStep {
    /// The focused DOM node's `id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<u32>,
    /// The focused DOM node's `backendNodeId`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_id: Option<u64>,
}

/// Deserialises a field that is present, keeping an explicit `null` as `Some(Value::Null)`, so a
/// round trip writes back exactly what it read. An absent field falls back to `None` through
/// `#[serde(default)]`.
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

/// Why [`read`] rejected a snapshot.
#[derive(Debug)]
pub enum ReadError {
    /// The text is not JSON. The `serde_json` message carries the line and column.
    Json(serde_json::Error),
    /// The JSON has no `schema` field equal to [`SCHEMA_ID`].
    NotASnapshot,
    /// `version.major` is not [`SUPPORTED_MAJOR`].
    UnsupportedMajor { found: u64 },
    /// `capture.engine` and `payload.engine` name different engines.
    EngineMismatch { capture: Engine, payload: Engine },
    /// A field is missing or has the wrong type or value. `path` is dotted, with indexes, such as
    /// `payload.dom[2].kind`; it is empty when the failure is at the top level.
    Field { path: String, message: String },
}

impl fmt::Display for ReadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Json(error) => write!(formatter, "snapshot is not valid JSON: {error}"),
            ReadError::NotASnapshot => write!(
                formatter,
                "not a spokenpath snapshot: the `schema` field must be \"{SCHEMA_ID}\""
            ),
            ReadError::UnsupportedMajor { found } => write!(
                formatter,
                "snapshot major version {found} is not supported; this reader reads major version {SUPPORTED_MAJOR}"
            ),
            ReadError::EngineMismatch { capture, payload } => write!(
                formatter,
                "capture.engine is `{capture}` but payload.engine is `{payload}`; they must name the same engine"
            ),
            ReadError::Field { path, message } if path.is_empty() => {
                write!(formatter, "invalid snapshot: {message}")
            }
            ReadError::Field { path, message } => {
                write!(formatter, "invalid snapshot at `{path}`: {message}")
            }
        }
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ReadError::Json(error) => Some(error),
            _ => None,
        }
    }
}

/// The major version alone, read before anything else, so a future major whose `version` has
/// another shape is still reported as an unsupported major.
#[derive(Deserialize)]
struct MajorOnly {
    version: Major,
}

#[derive(Deserialize)]
struct Major {
    major: u64,
}

/// The `engine` tag of `payload`.
#[derive(Deserialize)]
struct EngineTag {
    engine: Engine,
}

/// Everything in [`Snapshot`] but the payload's body. The reader deserialises the body on its own,
/// by engine, because serde buffers an internally tagged enum before choosing its variant, and
/// the buffering loses the path of a failing field.
#[derive(Deserialize)]
struct Envelope {
    schema: SchemaId,
    version: Version,
    producer: Producer,
    capture: Capture,
    volatile: Volatile,
    sources: Vec<Source>,
    spans: Vec<Span>,
    diagnostics: Vec<Diagnostic>,
    payload: EngineTag,
}

/// Reads a snapshot from JSON text.
///
/// The reader checks `schema` and `version` first, rejects a major version other than
/// [`SUPPORTED_MAJOR`], accepts any minor version, and ignores fields it does not know, except in
/// accessibility nodes, where they are kept.
///
/// # Errors
///
/// Returns a [`ReadError`] when the text is not JSON, is not a spokenpath snapshot, has an
/// unsupported major version, names two different engines, lists a source path that could leave
/// the capture root, or has a missing or malformed field.
pub fn read(json: &str) -> Result<Snapshot, ReadError> {
    let value: Value = serde_json::from_str(json).map_err(ReadError::Json)?;
    if value.get("schema").and_then(Value::as_str) != Some(SCHEMA_ID) {
        return Err(ReadError::NotASnapshot);
    }
    let MajorOnly { version } = deserialize_at("", &value)?;
    if version.major != u64::from(SUPPORTED_MAJOR) {
        return Err(ReadError::UnsupportedMajor {
            found: version.major,
        });
    }
    if let Some(payload) = value.get("payload")
        && !payload.is_object()
    {
        return Err(ReadError::Field {
            path: "payload".to_owned(),
            message: "expected an object with an `engine` field".to_owned(),
        });
    }
    let envelope: Envelope = deserialize_at("", &value)?;
    if let Some(index) = envelope
        .sources
        .iter()
        .position(|source| !is_source_path(&source.path))
    {
        return Err(ReadError::Field {
            path: format!("sources[{index}].path"),
            message: "expected a path relative to the capture root: `/`-separated segments, none of them empty, `.` or `..`, with no `\\`, `:` or NUL".to_owned(),
        });
    }
    let engine = envelope.payload.engine;
    if envelope.capture.engine != engine {
        return Err(ReadError::EngineMismatch {
            capture: envelope.capture.engine,
            payload: engine,
        });
    }
    // Present, because the envelope's own `payload` field deserialised from it.
    let body = value.get("payload").unwrap_or(&Value::Null);
    let payload = match engine {
        Engine::Chromium => Payload::Chromium(deserialize_at("payload", body)?),
        Engine::SafariWebdriver => {
            let payload: SafariWebdriverPayload = deserialize_at("payload", body)?;
            check_safari_elements(&payload, &envelope.spans)?;
            Payload::SafariWebdriver(payload)
        }
        Engine::SafariAx => Payload::SafariAx(deserialize_at("payload", body)?),
    };
    Ok(Snapshot {
        schema: envelope.schema,
        version: envelope.version,
        producer: envelope.producer,
        capture: envelope.capture,
        volatile: envelope.volatile,
        sources: envelope.sources,
        spans: envelope.spans,
        diagnostics: envelope.diagnostics,
        payload,
    })
}

/// Rejects a Safari element that holds both or neither of an answer and its error, or whose span
/// is not in `spans`.
fn check_safari_elements(
    payload: &SafariWebdriverPayload,
    spans: &[Span],
) -> Result<(), ReadError> {
    let span_ids: BTreeSet<u32> = spans.iter().map(|span| span.id).collect();
    for (index, element) in payload.elements.iter().enumerate() {
        let path = format!("payload.elements[{index}]");
        if element.role_answer().is_none() {
            return Err(ReadError::Field {
                path,
                message: "expected exactly one of `role` and `role_error`".to_owned(),
            });
        }
        if element.label_answer().is_none() {
            return Err(ReadError::Field {
                path,
                message: "expected exactly one of `label` and `label_error`".to_owned(),
            });
        }
        if !span_ids.contains(&element.span) {
            return Err(ReadError::Field {
                path: format!("{path}.span"),
                message: format!("no span has the id {}", element.span),
            });
        }
    }
    Ok(())
}

/// Deserialises `value`, which sits at `prefix` in the snapshot, reporting a failure with its full
/// dotted path.
fn deserialize_at<T: DeserializeOwned>(prefix: &str, value: &Value) -> Result<T, ReadError> {
    serde_path_to_error::deserialize(value).map_err(|error| {
        let inner = error.path().to_string();
        let path = match (prefix, inner.as_str()) {
            (_, ".") => prefix.to_owned(),
            ("", _) => inner,
            _ if inner.starts_with('[') => format!("{prefix}{inner}"),
            _ => format!("{prefix}.{inner}"),
        };
        ReadError::Field {
            path,
            message: error.into_inner().to_string(),
        }
    })
}

/// The JSON Schema (draft 2020-12) for [`Snapshot`], as `cargo xtask schema` writes it to
/// `schema/snapshot.v0.json`.
///
/// Optional fields are written only when present, so the schema allows them to be absent but not
/// `null`. A `$ref` never has sibling keywords: it moves into an `allOf`, because the TypeScript
/// generator drops the referenced schema when a `$ref` sits beside other keywords.
#[must_use]
pub fn json_schema() -> Value {
    schema_settings()
        .into_generator()
        .into_root_schema_for::<Snapshot>()
        .to_value()
}

/// The schema settings every committed spokenpath JSON Schema is generated with: draft 2020-12,
/// optional properties narrowed to their non-null form, and no `$ref` with sibling keywords, as
/// [`json_schema`] describes.
#[must_use]
pub fn schema_settings() -> schemars::generate::SchemaSettings {
    schemars::generate::SchemaSettings::draft2020_12().with(|settings| {
        let remove_null: fn(&mut Schema) = remove_null_from_optional;
        settings.transforms.push(Box::new(remove_null));
        settings
            .transforms
            .push(Box::new(schemars::transform::RemoveRefSiblings::default()));
    })
}

/// Narrows every optional property's schema to its non-null form. schemars allows `null` for an
/// `Option` because serde reads `null` as `None`, but writers omit the field instead, and the
/// generated TypeScript types follow the schema.
fn remove_null_from_optional(schema: &mut Schema) {
    if let Some(object) = schema.as_object_mut() {
        object.values_mut().for_each(remove_null_below);
        narrow_optional_properties(object);
    }
}

fn remove_null_below(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.values_mut().for_each(remove_null_below);
            narrow_optional_properties(object);
        }
        Value::Array(items) => items.iter_mut().for_each(remove_null_below),
        _ => {}
    }
}

/// Strips `null` from the schema of each property of `object` that is not `required`.
fn narrow_optional_properties(object: &mut Map<String, Value>) {
    let required: Vec<Value> = object
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(Value::Object(properties)) = object.get_mut("properties") {
        for (name, property) in properties.iter_mut() {
            if !required.iter().any(|required| required == name.as_str()) {
                strip_null(property);
            }
        }
    }
}

/// Removes the `null` alternative from one schema: `"type": [T, "null"]` becomes `"type": T`, and
/// `anyOf: [X, {"type": "null"}]` becomes `X`, keeping the property's own keywords such as its
/// description.
fn strip_null(property: &mut Value) {
    let Value::Object(object) = property else {
        return;
    };
    if let Some(Value::Array(types)) = object.get_mut("type") {
        types.retain(|kind| kind != "null");
        if let [only] = types.as_slice() {
            let only = only.clone();
            object.insert("type".to_owned(), only);
        }
        return;
    }
    let Some(Value::Array(alternatives)) = object.get_mut("anyOf") else {
        return;
    };
    let before = alternatives.len();
    alternatives
        .retain(|alternative| alternative.get("type").and_then(Value::as_str) != Some("null"));
    if alternatives.len() == before {
        return;
    }
    if let [Value::Object(only)] = alternatives.as_slice() {
        let only = only.clone();
        object.remove("anyOf");
        for (key, value) in only {
            object.entry(key).or_insert(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use serde_json::{Value, json};

    use super::{
        CURRENT_VERSION, Engine, Payload, ReadError, SCHEMA_ID, WebdriverAnswer, json_schema, read,
    };

    fn fixtures_directory() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/snapshots")
    }

    fn fixture(name: &str) -> String {
        let path = fixtures_directory().join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()))
    }

    fn basic() -> Value {
        serde_json::from_str(&fixture("chromium-basic.json")).expect("the fixture is JSON")
    }

    fn read_value(value: &Value) -> Result<super::Snapshot, ReadError> {
        read(&value.to_string())
    }

    /// Reads `name`, writes it back, and returns both as JSON values.
    fn round_trip(name: &str) -> (Value, Value) {
        let text = fixture(name);
        let snapshot = read(&text).unwrap_or_else(|error| panic!("{name} reads: {error}"));
        let written = serde_json::to_value(&snapshot).expect("a snapshot serialises");
        let input = serde_json::from_str(&text).expect("the fixture is JSON");
        (input, written)
    }

    #[test]
    fn every_fixture_reads_and_round_trips_unchanged() {
        let mut names: Vec<String> = std::fs::read_dir(fixtures_directory())
            .expect("fixtures/snapshots exists")
            .map(|entry| {
                entry
                    .expect("directory entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| {
                Path::new(name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            })
            .collect();
        names.sort();
        assert!(
            names.contains(&"chromium-basic.json".to_owned()),
            "{names:?}"
        );
        for name in &names {
            let (input, written) = round_trip(name);
            assert_eq!(written, input, "{name} changes in a round trip");
        }
    }

    #[test]
    fn snapshot_written_by_the_typescript_types_reads_and_round_trips() {
        let (input, written) = round_trip("chromium-from-typescript.json");
        assert_eq!(written, input);
    }

    #[test]
    fn current_version_is_zero_one() {
        assert_eq!((CURRENT_VERSION.major, CURRENT_VERSION.minor), (0, 1));
        assert_eq!(SCHEMA_ID, "spokenpath-snapshot");
    }

    #[test]
    fn major_version_one_is_rejected_naming_both_majors() {
        let mut value = basic();
        value["version"]["major"] = json!(1);
        let error = read_value(&value).expect_err("major 1 is rejected");
        assert!(
            matches!(error, ReadError::UnsupportedMajor { found: 1 }),
            "{error:?}"
        );
        insta::assert_snapshot!(error, @"snapshot major version 1 is not supported; this reader reads major version 0");
    }

    #[test]
    fn major_version_is_checked_before_the_rest_of_the_snapshot() {
        let error =
            read(r#"{"schema": "spokenpath-snapshot", "version": {"major": 1, "minor": 0}}"#)
                .expect_err("major 1 is rejected");
        assert!(
            matches!(error, ReadError::UnsupportedMajor { found: 1 }),
            "{error:?}"
        );
    }

    #[test]
    fn major_version_beyond_u32_is_rejected_as_unsupported() {
        let mut value = basic();
        value["version"]["major"] = json!(5_000_000_000_u64);
        let error = read_value(&value).expect_err("a huge major is rejected");
        assert!(
            matches!(
                error,
                ReadError::UnsupportedMajor {
                    found: 5_000_000_000
                }
            ),
            "{error:?}"
        );
    }

    #[test]
    fn future_major_without_a_minor_is_rejected_as_unsupported() {
        let mut value = basic();
        value["version"] = json!({"major": 1});
        let error = read_value(&value).expect_err("a future major is rejected");
        assert!(
            matches!(error, ReadError::UnsupportedMajor { found: 1 }),
            "{error:?}"
        );
    }

    #[test]
    fn missing_minor_under_the_supported_major_names_version_minor() {
        let mut value = basic();
        value["version"] = json!({"major": 0});
        let error = read_value(&value).expect_err("a missing minor is rejected");
        insta::assert_snapshot!(error, @"invalid snapshot at `version`: missing field `minor`");
    }

    #[test]
    fn non_object_payload_names_payload() {
        let mut value = basic();
        value["payload"] = json!("chromium");
        let error = read_value(&value).expect_err("a string payload is rejected");
        insta::assert_snapshot!(error, @"invalid snapshot at `payload`: expected an object with an `engine` field");
    }

    /// The source-path cases the Rust reader and the TypeScript validator share.
    fn source_path_cases() -> (Vec<String>, Vec<String>) {
        let path = fixtures_directory().join("../source-paths.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()));
        let cases: Value = serde_json::from_str(&text).expect("the cases are JSON");
        let list = |key: &str| -> Vec<String> {
            cases[key]
                .as_array()
                .expect("a list of paths")
                .iter()
                .map(|path| path.as_str().expect("a path string").to_owned())
                .collect()
        };
        (list("accepted"), list("rejected"))
    }

    #[test]
    fn source_paths_relative_to_the_capture_root_are_accepted() {
        let (accepted, _) = source_path_cases();
        assert!(accepted.contains(&"pages/basic.html".to_owned()));
        for path in accepted {
            let mut value = basic();
            value["sources"][0]["path"] = json!(path);
            read_value(&value).unwrap_or_else(|error| panic!("{path:?} is accepted: {error}"));
        }
    }

    #[test]
    fn source_paths_that_could_leave_the_capture_root_are_rejected() {
        let (_, rejected) = source_path_cases();
        for path in ["../x", "/x", "a/../b", "a\\b", "C:/x", "", "a/\0b"] {
            assert!(
                rejected.contains(&path.to_owned()),
                "{path:?} is a shared case"
            );
        }
        for path in rejected {
            let mut value = basic();
            value["sources"][0]["path"] = json!(path);
            let error = read_value(&value).expect_err("an escaping path is rejected");
            assert!(
                error.to_string().contains("`sources[0].path`"),
                "{path:?}: {error}"
            );
        }
    }

    #[test]
    fn schema_pattern_and_reader_state_the_same_source_path_rule() {
        let schema = json_schema();
        let pattern = &schema["$defs"]["Source"]["properties"]["path"]["pattern"];
        assert_eq!(pattern, &json!(super::SOURCE_PATH_PATTERN));
    }

    #[test]
    fn minor_version_ninety_nine_is_accepted() {
        let mut value = basic();
        value["version"]["minor"] = json!(99);
        let snapshot = read_value(&value).expect("minor 99 reads");
        assert_eq!(snapshot.version.minor, 99);
    }

    #[test]
    fn unknown_top_level_field_is_ignored() {
        let mut value = basic();
        value["added_in_a_later_minor"] = json!({"anything": [1, 2]});
        read_value(&value).expect("an unknown top-level field is ignored");
    }

    #[test]
    fn unknown_dom_node_field_is_ignored() {
        let mut value = basic();
        value["payload"]["dom"][2]["layout"] = json!("block");
        read_value(&value).expect("an unknown DOM node field is ignored");
    }

    #[test]
    fn unknown_ax_fields_survive_the_round_trip() {
        let mut value = basic();
        value["payload"]["ax"][0]["futureField"] = json!({"nested": [true, null]});
        value["payload"]["ax"][3]["name"]["futureValueField"] = json!(7);
        value["payload"]["ax"][3]["name"]["sources"][2]["futureSourceField"] = json!("x");
        value["payload"]["ax"][7]["name"]["sources"][2]["nativeSourceValue"]["relatedNodes"][0]["futureRelatedField"] =
            json!(1.5);
        let snapshot = read_value(&value).expect("unknown AX fields read");
        let written = serde_json::to_value(&snapshot).expect("a snapshot serialises");
        assert_eq!(written, value);
        assert_eq!(
            written["payload"]["ax"][0]["chromeRole"]["value"],
            json!(144)
        );
        assert_eq!(
            written["payload"]["ax"][0]["frameId"],
            json!("ABF6B215B2CAFC3AC52B47698CEB701A")
        );
    }

    #[test]
    fn explicit_null_ax_value_survives_the_round_trip() {
        let mut value = basic();
        value["payload"]["ax"][0]["role"]["value"] = Value::Null;
        let snapshot = read_value(&value).expect("a null AX value reads");
        assert_eq!(serde_json::to_value(&snapshot).expect("serialises"), value);
    }

    #[test]
    fn wrong_schema_identifier_is_not_a_snapshot() {
        let mut value = basic();
        value["schema"] = json!("something-else");
        let error = read_value(&value).expect_err("a wrong identifier is rejected");
        assert!(matches!(error, ReadError::NotASnapshot), "{error:?}");
        insta::assert_snapshot!(error, @r#"not a spokenpath snapshot: the `schema` field must be "spokenpath-snapshot""#);
    }

    #[test]
    fn missing_schema_identifier_is_not_a_snapshot() {
        let error = read("[1, 2]").expect_err("an array is not a snapshot");
        assert!(matches!(error, ReadError::NotASnapshot), "{error:?}");
    }

    #[test]
    fn missing_version_is_a_field_error_naming_version() {
        let mut value = basic();
        value.as_object_mut().expect("an object").remove("version");
        let error = read_value(&value).expect_err("a missing version is rejected");
        insta::assert_snapshot!(error, @"invalid snapshot: missing field `version`");
    }

    #[test]
    fn malformed_minor_version_is_a_field_error_naming_it() {
        let mut value = basic();
        value["version"]["minor"] = json!("one");
        let error = read_value(&value).expect_err("a string minor is rejected");
        assert!(error.to_string().contains("`version.minor`"), "{error}");
    }

    #[test]
    fn wrong_dom_kind_type_names_its_path() {
        let mut value = basic();
        value["payload"]["dom"][2]["kind"] = json!(7);
        let error = read_value(&value).expect_err("a numeric kind is rejected");
        insta::assert_snapshot!(error, @r#"invalid snapshot at `payload.dom[2].kind`: invalid type: integer `7`, expected string or map"#);
    }

    #[test]
    fn failing_ax_field_names_its_path() {
        let mut value = basic();
        value["payload"]["ax"][3]["name"]["type"] = json!(false);
        let error = read_value(&value).expect_err("a boolean value type is rejected");
        assert!(
            error.to_string().contains("`payload.ax[3].name.type`"),
            "{error}"
        );
    }

    #[test]
    fn unknown_capture_engine_names_the_field() {
        let mut value = basic();
        value["capture"]["engine"] = json!("firefox");
        let error = read_value(&value).expect_err("an unknown engine is rejected");
        insta::assert_snapshot!(error, @"invalid snapshot at `capture.engine`: unknown variant `firefox`, expected one of `chromium`, `safari-webdriver`, `safari-ax`");
    }

    #[test]
    fn engine_mismatch_names_both_fields() {
        let mut value = basic();
        value["capture"]["engine"] = json!("safari-ax");
        let error = read_value(&value).expect_err("a mismatch is rejected");
        assert!(
            matches!(
                error,
                ReadError::EngineMismatch {
                    capture: Engine::SafariAx,
                    payload: Engine::Chromium
                }
            ),
            "{error:?}"
        );
        insta::assert_snapshot!(error, @"capture.engine is `safari-ax` but payload.engine is `chromium`; they must name the same engine");
    }

    #[test]
    fn invalid_json_reports_line_and_column() {
        let error = read("{\n  \"schema\": \n}").expect_err("invalid JSON is rejected");
        assert!(matches!(error, ReadError::Json(_)), "{error:?}");
        insta::assert_snapshot!(error, @"snapshot is not valid JSON: expected value at line 3 column 1");
    }

    #[test]
    fn deeply_nested_input_is_an_error_not_a_crash() {
        let text = format!(
            r#"{{"schema": "spokenpath-snapshot", "x": {}{}}}"#,
            "[".repeat(100_000),
            "]".repeat(100_000)
        );
        let error = read(&text).expect_err("nesting beyond the recursion limit is rejected");
        assert!(error.to_string().contains("recursion limit"), "{error}");
    }

    #[test]
    fn safari_envelopes_with_empty_payloads_read() {
        for engine in [Engine::SafariWebdriver, Engine::SafariAx] {
            let mut value = basic();
            value["capture"]["engine"] = json!(engine.as_str());
            value["payload"] = json!({"engine": engine.as_str()});
            let snapshot = read_value(&value).expect("a Safari envelope reads");
            assert_eq!(snapshot.payload.engine(), engine);
            assert!(!matches!(snapshot.payload, Payload::Chromium(_)));
            assert_eq!(serde_json::to_value(&snapshot).expect("serialises"), value);
        }
    }

    /// `chromium-basic.json` as a minor-1 Safari `WebDriver` snapshot whose elements are `elements`.
    fn safari_with(elements: &Value) -> Value {
        let mut value = basic();
        value["version"]["minor"] = json!(1);
        value["capture"]["engine"] = json!("safari-webdriver");
        value["payload"] = json!({
            "engine": "safari-webdriver",
            "elements": elements,
            "safari_build": "22625.1.29.11.28",
            "macos_version": "27.0.1",
            "macos_build": "26A434",
        });
        value
    }

    #[test]
    fn safari_elements_with_answers_and_errors_read_and_round_trip() {
        let value = safari_with(&json!([
            { "span": 1, "role": "heading", "label": "" },
            {
                "span": 2,
                "role_error": { "error": "unknown error", "message": "" },
                "label": "Email",
            },
        ]));
        let snapshot = read_value(&value).expect("a Safari snapshot reads");
        let Payload::SafariWebdriver(payload) = &snapshot.payload else {
            panic!("a Safari payload: {:?}", snapshot.payload);
        };
        assert_eq!(payload.elements.len(), 2);
        assert_eq!(
            payload.elements[0].role_answer(),
            Some(WebdriverAnswer::Value("heading"))
        );
        assert!(matches!(
            payload.elements[1].role_answer(),
            Some(WebdriverAnswer::Error(error)) if error.error == "unknown error"
        ));
        assert_eq!(payload.macos_build.as_deref(), Some("26A434"));
        assert_eq!(serde_json::to_value(&snapshot).expect("serialises"), value);
    }

    #[test]
    fn safari_element_with_both_or_neither_answer_names_its_entry() {
        let error = json!({ "error": "unknown error", "message": "" });
        for (element, says) in [
            (
                json!({ "span": 1, "label": "x" }),
                "`role` and `role_error`",
            ),
            (
                json!({ "span": 1, "role": "x", "role_error": error, "label": "x" }),
                "`role` and `role_error`",
            ),
            (
                json!({ "span": 1, "role": "x" }),
                "`label` and `label_error`",
            ),
            (
                json!({ "span": 1, "role": "x", "label": "x", "label_error": error }),
                "`label` and `label_error`",
            ),
        ] {
            let value = safari_with(&json!([{ "span": 0, "role": "", "label": "" }, element]));
            let message = read_value(&value)
                .expect_err("an ambiguous entry is rejected")
                .to_string();
            assert!(
                message.starts_with(
                    "invalid snapshot at `payload.elements[1]`: expected exactly one of "
                ),
                "{message}"
            );
            assert!(message.contains(says), "{message}");
        }
    }

    #[test]
    fn safari_element_naming_no_span_is_rejected() {
        let value = safari_with(&json!([{ "span": 99, "role": "", "label": "" }]));
        let error = read_value(&value).expect_err("an unknown span is rejected");
        insta::assert_snapshot!(error, @"invalid snapshot at `payload.elements[0].span`: no span has the id 99");
    }

    #[test]
    fn malformed_safari_error_names_its_path() {
        let value = safari_with(&json!([{ "span": 0, "role_error": { "error": 3 }, "label": "" }]));
        let error = read_value(&value).expect_err("a numeric error code is rejected");
        assert!(
            error
                .to_string()
                .contains("`payload.elements[0].role_error.error`"),
            "{error}"
        );
    }

    #[test]
    fn schema_states_the_identifier_and_major_as_constants() {
        let schema = json_schema();
        assert_eq!(schema["$defs"]["SchemaId"]["const"], json!(SCHEMA_ID));
        assert_eq!(
            schema["$defs"]["Version"]["properties"]["major"]["const"],
            json!(0)
        );
    }

    #[test]
    fn schema_allows_optional_fields_to_be_absent_but_not_null() {
        let text = json_schema().to_string();
        assert!(
            !text.contains(r#""null""#),
            "the schema admits null somewhere"
        );
    }
}
