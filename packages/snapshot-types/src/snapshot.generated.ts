// Generated from schema/snapshot.v0.json; do not edit.
// Regenerate with `pnpm --filter @spokenpath/snapshot-types generate`.

/**
 * The emulated `prefers-color-scheme`.
 */
export type ColorScheme = "light" | "dark" | "no-preference";
/**
 * The engine a snapshot came from.
 */
export type Engine = "chromium" | "safari-webdriver" | "safari-ax";
/**
 * The emulated `prefers-reduced-motion`.
 */
export type ReducedMotion = "reduce" | "no-preference";
/**
 * The engine-specific part of the snapshot, tagged by `engine`.
 */
export type Payload =
  | (ChromiumPayload & {
      engine: "chromium";
    })
  | (SafariWebdriverPayload & {
      engine: "safari-webdriver";
    })
  | (SafariAxPayload & {
      engine: "safari-ax";
    });
/**
 * The type of a DOM node.
 */
export type DomNodeKind =
  | "document"
  | "doctype"
  | "element"
  | "text"
  | "comment"
  | "cdata-section"
  | "processing-instruction"
  | "document-fragment";
/**
 * Where an element came from.
 */
export type Origin = "source" | "implied" | "script" | "library" | "unknown";
/**
 * The mode of a shadow root.
 */
export type ShadowMode = "open" | "closed" | "user-agent";
/**
 * The fixed identifier `spokenpath-snapshot`.
 */
export type SchemaId = "spokenpath-snapshot";
/**
 * The encoding of a source file; only UTF-8 is supported.
 */
export type Encoding = "utf-8";

/**
 * One capture of one page in one engine: the envelope that every engine shares, plus the
 * engine-specific payload.
 */
export interface Snapshot {
  capture: Capture;
  /**
   * Problems the capture noticed and recorded rather than failing on.
   */
  diagnostics: Diagnostic[];
  payload: Payload;
  producer: Producer;
  /**
   * Identifies the file as a spokenpath snapshot.
   */
  schema: SchemaId;
  /**
   * The source files that spans point into.
   */
  sources: Source[];
  /**
   * Locations of tagged start tags in the source files.
   */
  spans: Span[];
  version: Version;
  volatile: Volatile;
}
/**
 * The engine and the conditions the page was captured under.
 */
export interface Capture {
  color_scheme: ColorScheme;
  /**
   * The engine the snapshot came from; it equals `payload.engine`.
   */
  engine: Engine;
  /**
   * The browser version, as the browser reports it.
   */
  engine_version: string;
  reduced_motion: ReducedMotion;
  /**
   * The name of the strategy that decided the page had settled; the capture backends define
   * the values.
   */
  settle: string;
  /**
   * The version of the tool that drove the browser, for example Playwright.
   */
  tool_version: string;
  /**
   * The URL the page was loaded from.
   */
  url: string;
  viewport: Viewport;
}
/**
 * The viewport, in CSS pixels.
 */
export interface Viewport {
  device_pixel_ratio: number;
  height: number;
  width: number;
}
/**
 * A problem the capture recorded.
 */
export interface Diagnostic {
  /**
   * A stable, machine-readable code.
   */
  code: string;
  message: string;
  /**
   * The `id` of the DOM node the problem concerns.
   */
  node?: number;
}
/**
 * What Chromium exposes for the page, as the `DevTools` protocol delivers it.
 */
export interface ChromiumPayload {
  /**
   * The nodes of `Accessibility.getFullAXTree`.
   */
  ax: AxNode[];
  /**
   * One `Accessibility.getPartialAXTree` result per DOM element that has no node in the full
   * tree, kept apart from `ax`.
   */
  ax_forced: AxForced[];
  /**
   * Every DOM node, flat, with the document first.
   */
  dom: DomNode[];
  /**
   * The raw `DOMSnapshot.captureSnapshot` response, with computed `display` and `visibility`,
   * kept as delivered.
   */
  dom_snapshot?: {
    [k: string]: unknown;
  };
  focus: Focus;
}
/**
 * An accessibility node: a typed subset of the protocol's `AXNode`, in the protocol's names.
 * Name provenance (`name.sources`) and exclusion reasons (`ignoredReasons`) are optional because
 * not every engine provides them.
 */
export interface AxNode {
  backendDOMNodeId?: number;
  childIds?: string[];
  description?: AxValue;
  ignored: boolean;
  ignoredReasons?: AxProperty[];
  name?: AxValue;
  nodeId: string;
  parentId?: string;
  properties?: AxProperty[];
  role?: AxValue;
  value?: AxValue;
  [k: string]: unknown;
}
/**
 * The protocol's `AXValue`. `type` stays a string so a value type the protocol adds later is kept
 * rather than rejected.
 */
export interface AxValue {
  relatedNodes?: AxRelatedNode[];
  /**
   * Where the value came from, in the order the engine tried them (name provenance).
   */
  sources?: AxValueSource[];
  type: string;
  value?: unknown;
  [k: string]: unknown;
}
/**
 * The protocol's `AXRelatedNode`.
 */
export interface AxRelatedNode {
  backendDOMNodeId: number;
  idref?: string;
  text?: string;
  [k: string]: unknown;
}
/**
 * The protocol's `AXValueSource`: one source the engine tried for a value.
 */
export interface AxValueSource {
  attribute?: string;
  attributeValue?: AxValue;
  nativeSource?: string;
  nativeSourceValue?: AxValue;
  superseded?: boolean;
  type: string;
  value?: AxValue;
  [k: string]: unknown;
}
/**
 * A named value: an `AXProperty`, or an entry of `ignoredReasons`.
 */
export interface AxProperty {
  name: string;
  value: AxValue;
  [k: string]: unknown;
}
/**
 * The partial-tree result for one DOM element missing from the full tree.
 */
export interface AxForced {
  /**
   * The DOM node's `backendNodeId`, which the query used.
   */
  backend_id: number;
  /**
   * The DOM node's `id`.
   */
  node: number;
  /**
   * The response's node list, as delivered.
   */
  nodes: AxNode[];
}
/**
 * One DOM node.
 */
export interface DomNode {
  /**
   * The attributes in their DOM order, with `data-spokenpath-src` removed.
   */
  attributes: Attribute[];
  /**
   * The protocol's `backendNodeId`.
   */
  backend_id: number;
  /**
   * The children's `id`s, in DOM order. A shadow host lists its shadow root first.
   */
  children: number[];
  /**
   * Snapshot-local id.
   */
  id: number;
  kind: DomNodeKind;
  /**
   * The namespace URI of an element.
   */
  namespace?: string;
  /**
   * Where an element came from; set on element nodes.
   */
  origin?: Origin;
  /**
   * The parent's `id`; absent only for the root. A shadow root's parent is its host.
   */
  parent?: number;
  /**
   * The mode of a shadow root.
   */
  shadow?: ShadowMode;
  /**
   * The `id` of the span locating the element's start tag in the source.
   */
  span?: number;
  /**
   * The tag name of an element, as the browser reports it.
   */
  tag?: string;
  /**
   * The text of a text-bearing node.
   */
  text?: Text;
}
/**
 * One attribute of an element.
 */
export interface Attribute {
  name: string;
  value: string;
}
/**
 * The text of a text-bearing node.
 */
export interface Text {
  /**
   * True when the capture shortened `value`.
   */
  truncated: boolean;
  value: string;
}
/**
 * The walk of focus by repeated Tab presses.
 */
export interface Focus {
  /**
   * The index in `steps` of the first step whose element an earlier step already focused.
   */
  cycle_start?: number;
  /**
   * One step per Tab press, in order.
   */
  steps: FocusStep[];
  /**
   * True when the walk stopped at its step limit.
   */
  truncated: boolean;
}
/**
 * The element focused after one Tab press; both fields are absent when focus left the document.
 */
export interface FocusStep {
  /**
   * The focused DOM node's `backendNodeId`.
   */
  backend_id?: number;
  /**
   * The focused DOM node's `id`.
   */
  node?: number;
}
/**
 * The Safari payload captured through `WebDriver`. Its contents are added in later minor versions.
 */
export interface SafariWebdriverPayload {}
/**
 * The Safari payload captured from the macOS accessibility tree. Its contents are added in later
 * minor versions.
 */
export interface SafariAxPayload {}
/**
 * The program that wrote the snapshot.
 */
export interface Producer {
  name: string;
  version: string;
}
/**
 * A source file the page was built from.
 */
export interface Source {
  encoding: Encoding;
  /**
   * Snapshot-local id that spans refer to.
   */
  id: number;
  /**
   * The file's path relative to the capture root, with `/` separators.
   */
  path: string;
  /**
   * SHA-256 of the file's bytes, as 64 lowercase hex characters.
   */
  sha256: string;
}
/**
 * The location of one tagged start tag in a source file.
 */
export interface Span {
  /**
   * The 1-based column of the start tag's `<`, counted in Unicode scalar values from the start
   * of the line.
   */
  column: number;
  /**
   * The whole element, from its start tag to the end of its end tag, when the element has one.
   */
  element?: ByteRange;
  /**
   * Snapshot-local id that DOM nodes refer to.
   */
  id: number;
  /**
   * The 1-based line of the start tag's `<`.
   */
  line: number;
  /**
   * The `id` of the source file the ranges point into.
   */
  source: number;
  /**
   * The start tag, from its `<` to just past its `>`.
   */
  start_tag: ByteRange;
}
/**
 * A half-open range of UTF-8 byte offsets into a source file: `start` is the first byte and `end`
 * is one past the last.
 */
export interface ByteRange {
  /**
   * UTF-8 byte offset one past the last byte, from the start of the file.
   */
  end: number;
  /**
   * UTF-8 byte offset of the first byte, from the start of the file.
   */
  start: number;
}
/**
 * The format version. A reader rejects a major version it does not know and accepts any minor
 * version, because minor versions only add fields.
 */
export interface Version {
  major: 0;
  minor: number;
}
/**
 * Fields that differ on every run of the same capture. Golden comparisons exclude this whole
 * object.
 */
export interface Volatile {
  /**
   * When the capture ran, as an RFC 3339 timestamp in UTC.
   */
  captured_at: string;
}
