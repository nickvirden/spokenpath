// Reads the macOS accessibility (AX) tree of the Safari web area whose URL
// contains a given substring, and prints it as JSON on standard output. Only
// that web area is recorded; other Safari windows are passed over unread.
//
// Usage: swift ax-tree.swift <url-substring> <needle>
//   (or the same arguments to a binary built from it with swiftc)
//   <needle> is searched for in every string-valued attribute, to find any
//   attribute that exposes a data-* value directly.
//
// Exit status: 0 when the tree was read with the permission; 3 when the
// permission is missing, meaning AXIsProcessTrusted() is false or the first
// query fails with kAXErrorAPIDisabled (the attempt is still recorded, with
// the AX error code); 2 on bad arguments or when Safari is not running.
import AppKit
import ApplicationServices
import Foundation

let arguments = CommandLine.arguments
guard arguments.count == 3 else {
  FileHandle.standardError.write("usage: ax-tree.swift <url-substring> <needle>\n".data(using: .utf8)!)
  exit(2)
}
let urlSubstring = arguments[1]
let needle = arguments[2]
let trusted = AXIsProcessTrusted()

func attribute(_ element: AXUIElement, _ name: String) -> (value: CFTypeRef?, error: AXError) {
  var value: CFTypeRef?
  let error = AXUIElementCopyAttributeValue(element, name as CFString, &value)
  return (value, error)
}

func attributeNames(_ element: AXUIElement) -> (names: [String], error: AXError) {
  var names: CFArray?
  let error = AXUIElementCopyAttributeNames(element, &names)
  return ((names as? [String]) ?? [], error)
}

func children(_ element: AXUIElement) -> [AXUIElement] {
  (attribute(element, kAXChildrenAttribute).value as? [AXUIElement]) ?? []
}

// Strings, numbers, booleans and arrays of strings are recorded as JSON; any
// other value (element references, ranges, frames) is recorded by type only.
func jsonValue(_ value: CFTypeRef?) -> Any {
  guard let value else { return NSNull() }
  if let string = value as? String { return string }
  if CFGetTypeID(value) == CFBooleanGetTypeID() { return (value as! Bool) }
  if let number = value as? NSNumber { return number }
  if let url = value as? URL { return url.absoluteString }
  if let strings = value as? [String] { return strings }
  return "<\(CFCopyTypeIDDescription(CFGetTypeID(value)) as String)>"
}

func containsNeedle(_ value: CFTypeRef?) -> Bool {
  if let string = value as? String { return string.contains(needle) }
  if let strings = value as? [String] { return strings.contains { $0.contains(needle) } }
  return false
}

let recordedAttributes = [
  "AXRole", "AXSubrole", "AXRoleDescription", "AXTitle", "AXDescription", "AXValue", "AXHelp",
  "AXDOMIdentifier", "AXDOMClassList",
]

func record(_ element: AXUIElement, depth: Int, into nodes: inout [[String: Any]]) {
  let (names, namesError) = attributeNames(element)
  var node: [String: Any] = ["index": nodes.count, "depth": depth, "attributeNames": names]
  if namesError != .success { node["attributeNamesError"] = namesError.rawValue }
  for name in recordedAttributes {
    let (value, error) = attribute(element, name)
    if error == .success { node[name] = jsonValue(value) }
  }
  node["attributesContainingNeedle"] = names.filter { containsNeedle(attribute(element, $0).value) }
  nodes.append(node)
  for child in children(element) {
    record(child, depth: depth + 1, into: &nodes)
  }
}

// Walks windows to the web areas, descending only through non-web-area
// elements, and returns the first web area whose AXURL matches.
func findWebArea(_ root: AXUIElement) -> (match: AXUIElement?, webAreasSeen: Int, rootError: AXError) {
  var queue = [root]
  var seen = 0
  let rootError = attribute(root, kAXChildrenAttribute).error
  while !queue.isEmpty {
    let element = queue.removeFirst()
    let role = attribute(element, kAXRoleAttribute).value as? String
    if role == "AXWebArea" {
      seen += 1
      let url = attribute(element, "AXURL").value
      let urlString = (url as? URL)?.absoluteString ?? (url as? String) ?? ""
      if urlString.contains(urlSubstring) { return (element, seen, rootError) }
      continue
    }
    queue.append(contentsOf: children(element))
  }
  return (nil, seen, rootError)
}

guard
  let safari = NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.Safari").first
else {
  FileHandle.standardError.write("Safari is not running\n".data(using: .utf8)!)
  exit(2)
}
let app = AXUIElementCreateApplication(safari.processIdentifier)

// The AX error codes the probe can meet, by name, so the output states them
// as data rather than as bare numbers.
func errorName(_ error: AXError) -> String {
  switch error {
  case .success: return "kAXErrorSuccess"
  case .apiDisabled: return "kAXErrorAPIDisabled"
  case .cannotComplete: return "kAXErrorCannotComplete"
  case .noValue: return "kAXErrorNoValue"
  case .attributeUnsupported: return "kAXErrorAttributeUnsupported"
  case .invalidUIElement: return "kAXErrorInvalidUIElement"
  case .notImplemented: return "kAXErrorNotImplemented"
  default: return "AXError \(error.rawValue)"
  }
}

var attempts: [[String: Any]] = []
var apiDisabled = false
var populated: [[String: Any]] = []
var restore: [(String, CFTypeRef?)] = []

// Each attempt queries again; the later ones first wait, then set one
// app-level attribute that asks Safari to build its web accessibility tree.
// Only attributes on Safari's own application element are set, and each is
// put back afterwards.
let plan: [(trigger: String, delay: UInt32, set: String?)] = [
  ("first query", 0, nil),
  ("repeat after 1 s", 1, nil),
  ("set AXManualAccessibility on the Safari application", 1, "AXManualAccessibility"),
  ("set AXEnhancedUserInterface on the Safari application", 1, "AXEnhancedUserInterface"),
]
for step in plan {
  if !populated.isEmpty { break }
  sleep(step.delay)
  var entry: [String: Any] = ["trigger": step.trigger]
  if let name = step.set {
    let previous = attribute(app, name)
    let error = AXUIElementSetAttributeValue(app, name as CFString, kCFBooleanTrue)
    entry["setError"] = error.rawValue
    entry["setErrorName"] = errorName(error)
    if error == .success { restore.append((name, previous.value)) }
    sleep(1)
  }
  let found = findWebArea(app)
  entry["applicationChildrenError"] = found.rootError.rawValue
  entry["applicationChildrenErrorName"] = errorName(found.rootError)
  entry["webAreasSeen"] = found.webAreasSeen
  entry["matchingWebAreaFound"] = found.match != nil
  var nodes: [[String: Any]] = []
  if let webArea = found.match {
    record(webArea, depth: 0, into: &nodes)
  }
  entry["nodeCount"] = nodes.count
  attempts.append(entry)
  if nodes.count > 1 { populated = nodes }
  // Without the permission every call fails the same way, so retrying or
  // setting attributes cannot change the answer.
  if found.rootError == .apiDisabled {
    apiDisabled = true
    break
  }
}
for (name, previous) in restore {
  AXUIElementSetAttributeValue(app, name as CFString, previous ?? kCFBooleanFalse)
}

// The permission counts as missing on either signal, never on an empty node
// count alone: an empty tree can also mean the page is not showing.
let permissionMissing = !trusted || apiDisabled
let os = ProcessInfo.processInfo.operatingSystemVersion
let output: [String: Any] = [
  "axIsProcessTrusted": trusted,
  "permission": permissionMissing ? "missing" : "granted",
  "macosVersion": "\(os.majorVersion).\(os.minorVersion).\(os.patchVersion)",
  "urlSubstring": urlSubstring,
  "needle": needle,
  "attempts": attempts,
  "populatedOnFirstQuery": (attempts.first?["nodeCount"] as? Int ?? 0) > 1,
  "nodeCount": populated.count,
  "nodes": populated,
]
let data = try JSONSerialization.data(withJSONObject: output, options: [.prettyPrinted, .sortedKeys])
FileHandle.standardOutput.write(data)
FileHandle.standardOutput.write("\n".data(using: .utf8)!)
exit(permissionMissing ? 3 : 0)
