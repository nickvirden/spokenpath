use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use spokenpath_snapshot::{Engine, Payload, Snapshot, read};

use super::adapt;
use crate::{
    AccessibleNode, AdaptError, Availability, ExposedNode, Model, PropertyValue, Role, SourceClass,
    TextPartKind, Tristate,
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn captures_directory() -> PathBuf {
    fixtures().join("snapshots/safari-ax")
}

/// Every capture under `fixtures/snapshots/safari-ax/`, relative to it, found by listing the
/// directories; `source-mapping.json` is a report, not a snapshot.
fn capture_names() -> Vec<PathBuf> {
    let root = captures_directory();
    let mut names = Vec::new();
    let mut directories = vec![root.clone()];
    while let Some(directory) = directories.pop() {
        let entries = std::fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("{} is listable: {error}", directory.display()));
        for entry in entries {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                directories.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "json")
                && path
                    .file_name()
                    .is_some_and(|name| name != "source-mapping.json")
            {
                names.push(
                    path.strip_prefix(&root)
                        .expect("under the root")
                        .to_path_buf(),
                );
            }
        }
    }
    names.sort();
    names
}

fn value_at(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is JSON: {error}", path.display()))
}

fn read_value(value: &Value) -> Snapshot {
    read(&value.to_string()).unwrap_or_else(|error| panic!("the snapshot reads: {error}"))
}

/// The byte length of each source of a capture, read from its page under `fixtures/pages/`.
fn source_lengths(name: &Path, snapshot: &Snapshot) -> BTreeMap<u32, u64> {
    let pages = fixtures()
        .join("pages")
        .canonicalize()
        .expect("fixtures/pages exists");
    let directory = pages.join(name.parent().unwrap_or(Path::new("")));
    snapshot
        .sources
        .iter()
        .map(|source| {
            let page = directory
                .join(&source.path)
                .canonicalize()
                .expect("the page exists");
            assert!(
                page.starts_with(&pages),
                "{} leaves fixtures/pages",
                page.display()
            );
            (source.id, std::fs::metadata(&page).expect("readable").len())
        })
        .collect()
}

fn adapted(name: &str) -> Model {
    let name = Path::new(name);
    let snapshot = read_value(&value_at(&captures_directory().join(name)));
    adapt(&snapshot, &source_lengths(name, &snapshot))
        .unwrap_or_else(|error| panic!("{} adapts: {error}", name.display()))
}

fn basic_value() -> Value {
    value_at(&fixtures().join("snapshots/safari-ax-basic.json"))
}

/// A string attribute value.
fn text(value: &str) -> Value {
    json!({ "type": "string", "value": value })
}

/// `safari-ax-basic.json` with its nodes replaced by a web area (node 0) whose only child is a node
/// with `attributes`, and no spans.
fn with_child(attributes: &Value) -> Snapshot {
    let mut value = basic_value();
    value["payload"]["nodes"] = json!([
        {
            "id": 0, "children": [1],
            "attributes": { "AXRole": text("AXWebArea"), "AXDescription": text("Page") }
        },
        { "id": 1, "parent": 0, "children": [], "attributes": attributes },
    ]);
    read_value(&value)
}

/// The model's node 1, the web area's child of [`with_child`].
fn child(attributes: &Value) -> AccessibleNode {
    let model = adapt(&with_child(attributes), &BTreeMap::new()).expect("adapts");
    model.nodes[1]
        .as_accessible()
        .expect("an accessible node")
        .clone()
}

fn assert_names_field(error: &AdaptError, path: &str) {
    let text = error.to_string();
    assert!(
        text.starts_with(&format!("invalid snapshot at `{path}`: ")),
        "expected an error at `{path}`, got: {text}"
    );
}

/// `safari-ax-basic.json` read, then its payload edited, so the edit can make what the reader
/// would itself reject.
fn adapt_edited(edit: impl FnOnce(&mut spokenpath_snapshot::SafariAxPayload)) -> AdaptError {
    let mut snapshot = read_value(&basic_value());
    let Payload::SafariAx(payload) = &mut snapshot.payload else {
        panic!("a Safari AX payload");
    };
    edit(payload);
    adapt(&snapshot, &BTreeMap::new()).expect_err("the edited snapshot is rejected")
}

#[test]
fn every_committed_capture_and_the_hand_written_one_adapt() {
    let names = capture_names();
    assert!(names.len() >= 46, "{names:?}");
    for name in &names {
        let model = adapted(name.to_str().expect("UTF-8"));
        assert_eq!(model.engine, Engine::SafariAx, "{}", name.display());
        assert!(!model.nodes.is_empty(), "{}", name.display());
        assert_eq!(model.excluded, Availability::NotAvailable);
        assert_eq!(model.focus, Availability::NotAvailable);
        let root = model.nodes[0]
            .as_accessible()
            .expect("the root is accessible");
        assert_eq!(root.parent, None);
        assert_eq!(
            root.role,
            Some(Role::Platform {
                role: "AXWebArea".to_owned(),
                subrole: None
            }),
            "{}",
            name.display()
        );
    }
    let snapshot = read_value(&basic_value());
    let model = adapt(&snapshot, &BTreeMap::new()).expect("safari-ax-basic.json adapts");
    assert_eq!(model.engine_build.as_deref(), Some("22625.1.29.11.28"));
    assert_eq!(model.os_version.as_deref(), Some("27.0.1"));
    assert_eq!(model.os_build.as_deref(), Some("26A434"));
}

#[test]
fn a_chromium_snapshot_is_an_unsupported_engine() {
    let path = fixtures().join("snapshots/chromium-basic.json");
    let snapshot = read_value(&value_at(&path));
    assert_eq!(
        adapt(&snapshot, &BTreeMap::new()),
        Err(AdaptError::UnsupportedEngine {
            expected: Engine::SafariAx,
            found: Engine::Chromium
        })
    );
}

#[test]
fn a_span_past_its_source_is_rejected_before_any_node_is_read() {
    let mut snapshot = read_value(&basic_value());
    if let Payload::SafariAx(payload) = &mut snapshot.payload {
        // A broken tree, which would fail later: the span check must fail first.
        payload.nodes.clear();
    }
    let error = adapt(&snapshot, &BTreeMap::from([(0, 3)])).expect_err("rejected");
    assert_names_field(&error, "spans[0].start_tag");
}

#[test]
fn each_broken_tree_is_rejected_at_its_dotted_path() {
    type Edit = fn(&mut spokenpath_snapshot::SafariAxPayload);
    let cases: [(&str, Edit, &str); 7] = [
        ("no nodes", |payload| payload.nodes.clear(), "payload.nodes"),
        (
            "a parent on node 0",
            |payload| payload.nodes[0].parent = Some(3),
            "payload.nodes[0].parent",
        ),
        (
            "a node with no parent",
            |payload| payload.nodes[3].parent = None,
            "payload.nodes[3].parent",
        ),
        (
            "a node listed by another parent",
            |payload| {
                payload.nodes[1].children.push(4);
            },
            "payload.nodes[1].children[1]",
        ),
        (
            "a node listed twice",
            |payload| payload.nodes[3].children.push(4),
            "payload.nodes[3].children[1]",
        ),
        (
            "node 0 listed as a child",
            |payload| payload.nodes[3].children.push(0),
            "payload.nodes[3].children[1]",
        ),
        (
            "a node its parent does not list",
            |payload| payload.nodes[0].children.retain(|&child| child != 5),
            "payload.nodes[5].parent",
        ),
    ];
    for (case, edit, path) in cases {
        let error = adapt_edited(edit);
        let text = error.to_string();
        assert!(
            text.starts_with(&format!("invalid snapshot at `{path}`: ")),
            "{case}: expected `{path}`, got {text}"
        );
    }
}

#[test]
fn a_parent_cycle_is_an_error_not_a_hang() {
    // Nodes 1 and 3 name each other as parent and child, cut off from node 0.
    let error = adapt_edited(|payload| {
        payload.nodes[0]
            .children
            .retain(|&child| child != 1 && child != 3);
        payload.nodes[1].parent = Some(3);
        payload.nodes[3].parent = Some(1);
        payload.nodes[1].children = vec![2, 3];
        payload.nodes[3].children = vec![4, 1];
    });
    assert!(error.to_string().contains("not one tree"), "{error}");
}

/// A chain of `depth` groups under the web area.
fn chain(depth: u32) -> Snapshot {
    let mut value = basic_value();
    value["spans"] = json!([]);
    let mut nodes =
        vec![json!({ "id": 0, "children": [1], "attributes": { "AXRole": text("AXWebArea") } })];
    for id in 1..=depth {
        nodes.push(json!({
            "id": id, "parent": id - 1,
            "children": if id == depth { json!([]) } else { json!([id + 1]) },
            "attributes": { "AXRole": text("AXGroup") }
        }));
    }
    value["payload"]["nodes"] = Value::Array(nodes);
    read_value(&value)
}

#[test]
fn a_deep_chain_adapts_without_overflowing_the_stack() {
    let model = adapt(&chain(100_000), &BTreeMap::new()).expect("the chain adapts");
    assert_eq!(model.nodes.len(), 100_001);
    assert_eq!(
        model
            .nodes
            .last()
            .and_then(ExposedNode::parent)
            .map(crate::NodeIndex::get),
        Some(99_999)
    );
}

#[test]
fn the_tree_keeps_safari_child_order_and_ignores_navigation_order() {
    let model = adapted("sign-in.json");
    let root = model.nodes[0].as_accessible().expect("accessible");
    let roles: Vec<String> = root
        .children
        .iter()
        .map(|&child| {
            model
                .node(child)
                .and_then(ExposedNode::as_accessible)
                .and_then(|node| node.role.as_ref())
                .map_or_else(String::new, |role| role.as_str().to_owned())
        })
        .collect();
    assert_eq!(roles, ["heading", "AXGroup", "AXGroup", "AXGroup"]);
    // The model is in pre-order: every node follows its parent.
    for (index, node) in model.indexed_nodes() {
        if let Some(parent) = node.parent() {
            assert!(parent < index);
        }
    }
    assert!(
        model
            .indexed_nodes()
            .all(|(_, node)| node.as_accessible().is_none_or(|node| node
                .properties
                .iter()
                .all(|property| property.name != "AXChildrenInNavigationOrder")))
    );
}

#[test]
fn untagged_leaf_static_text_is_a_text_run_and_tagged_static_text_is_not() {
    let model = adapted("sign-in.json");
    let runs: Vec<&str> = model
        .nodes
        .iter()
        .filter_map(ExposedNode::as_text_run)
        .map(|run| {
            assert_eq!(run.parts.len(), 1);
            assert_eq!(run.parts[0].kind, TextPartKind::Text);
            assert_eq!(run.parts[0].dom, None);
            assert_eq!(run.parts[0].source, SourceClass::Text);
            run.text.as_str()
        })
        .collect();
    assert_eq!(runs, ["Sign in", "Email", "Trouble signing in?"]);
    // The label carries a token, so it is an accessible node with its value.
    let label = model
        .nodes
        .iter()
        .filter_map(ExposedNode::as_accessible)
        .find(|node| matches!(&node.source, SourceClass::Span(span) if span.line == 10))
        .expect("the label");
    assert_eq!(
        label.role,
        Some(Role::Platform {
            role: "AXStaticText".to_owned(),
            subrole: None
        })
    );
    assert_eq!(label.value.as_deref(), Some("Email"));
    assert_eq!(label.name, None);

    // A text node with children, or a non-string value, stays an accessible node.
    let node = child(
        &json!({ "AXRole": text("AXStaticText"), "AXValue": { "type": "number", "value": 3 } }),
    );
    assert_eq!(node.source, SourceClass::Text);
}

#[test]
fn untagged_nodes_are_document_text_or_unknown() {
    let model = adapted("sign-in.json");
    assert_eq!(
        model.nodes[0].as_accessible().expect("root").source,
        SourceClass::Document
    );
    assert_eq!(
        child(&json!({ "AXRole": text("AXGroup") })).source,
        SourceClass::Unknown
    );
    assert_eq!(
        child(&json!({ "AXRole": { "type": "error", "code": -25205 } })).source,
        SourceClass::Unknown
    );
    for node in model.nodes.iter().filter_map(ExposedNode::as_accessible) {
        assert_eq!(
            (&node.dom, &node.role_origin, &node.set_position),
            (&None, &None, &None)
        );
    }
}

fn role_of(role: &str, subrole: Option<Value>) -> Option<Role> {
    let mut attributes = json!({ "AXRole": text(role) });
    if let Some(subrole) = subrole {
        attributes["AXSubrole"] = subrole;
    }
    child(&attributes).role
}

#[test]
fn the_role_table_maps_exactly_its_pairs() {
    let no_value = || Some(json!({ "type": "error", "code": -25212 }));
    for (role, subrole, token) in super::ARIA_ROLES {
        let subrole = subrole.map_or_else(no_value, |subrole| Some(text(subrole)));
        assert_eq!(
            role_of(role, subrole),
            Some(Role::Aria(token.to_owned())),
            "{role}"
        );
    }
    // An absent subrole reads as none, like the no-value error.
    assert_eq!(
        role_of("AXButton", None),
        Some(Role::Aria("button".to_owned()))
    );
    let platform = |role: &str, subrole: Option<&str>| {
        Some(Role::Platform {
            role: role.to_owned(),
            subrole: subrole.map(str::to_owned),
        })
    };
    for role in [
        "AXGroup",
        "AXList",
        "AXPopUpButton",
        "AXStaticText",
        "AXWebArea",
        "AXTable",
        "AXRow",
        "AXCell",
        "AXMenuItem",
        "AXIncrementor",
    ] {
        assert_eq!(role_of(role, no_value()), platform(role, None), "{role}");
    }
    assert_eq!(
        role_of("AXGroup", Some(text("AXApplicationGroup"))),
        platform("AXGroup", Some("AXApplicationGroup"))
    );
    // A listed role with an unlisted subrole keeps both as text.
    assert_eq!(
        role_of("AXButton", Some(text("AXToggle"))),
        platform("AXButton", Some("AXToggle"))
    );
    // A subrole read that failed otherwise makes the pair unknown.
    assert_eq!(
        role_of("AXButton", Some(json!({ "type": "error", "code": -25205 }))),
        platform("AXButton", None)
    );
    assert_eq!(
        child(&json!({ "AXRole": { "type": "error", "code": -25212 } })).role,
        None
    );
}

#[test]
fn the_name_is_the_title_then_the_description() {
    let name = |attributes: Value| child(&attributes).name.map(|name| name.text);
    assert_eq!(
        name(json!({ "AXTitle": text("T"), "AXDescription": text("D") })),
        Some("T".to_owned())
    );
    assert_eq!(
        name(json!({ "AXTitle": text(""), "AXDescription": text("D") })),
        Some("D".to_owned())
    );
    assert_eq!(
        name(json!({ "AXDescription": text("D") })),
        Some("D".to_owned())
    );
    assert_eq!(
        name(json!({ "AXTitle": text(""), "AXDescription": text("") })),
        None
    );
    let named = child(&json!({ "AXTitle": text("T"), "AXDescription": text("D") }));
    let name = named.name.expect("a name");
    assert_eq!(
        (name.sources, name.winner, name.from_hidden_content),
        (Availability::NotAvailable, None, None)
    );
    // The attribute the name did not take stays a property.
    let kept: Vec<&str> = named
        .properties
        .iter()
        .map(|property| property.name.as_str())
        .collect();
    assert_eq!(kept, ["AXDescription"]);
    assert!(
        adapted("names/aria-label.json")
            .nodes
            .iter()
            .filter_map(ExposedNode::as_accessible)
            .filter_map(|node| node.name.as_ref())
            .map(|name| name.text.as_str())
            .collect::<Vec<_>>()
            .contains(&"Aria label text")
    );
}

#[test]
fn help_is_the_description_and_a_non_empty_value_is_the_value() {
    let node = child(&json!({ "AXHelp": text("Help"), "AXValue": text("v") }));
    let description = node.description.expect("a description");
    assert_eq!(
        (description.text.as_str(), description.provenance),
        ("Help", None)
    );
    assert_eq!(node.value.as_deref(), Some("v"));
    let empty = child(&json!({ "AXHelp": text(""), "AXValue": text("") }));
    assert_eq!((empty.description, empty.value), (None, None));
    let kept: Vec<&str> = empty
        .properties
        .iter()
        .map(|property| property.name.as_str())
        .collect();
    assert_eq!(kept, ["AXHelp", "AXValue"]);
}

#[test]
fn boolean_and_token_states_fill_their_fields() {
    let node = child(&json!({
        "AXEnabled": { "type": "boolean", "value": false },
        "AXFocused": { "type": "boolean", "value": true },
        "AXSelected": { "type": "boolean", "value": true },
        "AXExpanded": { "type": "boolean", "value": false },
        "AXRequired": { "type": "boolean", "value": true },
        "AXInvalid": text("spelling"),
    }));
    let states = &node.states;
    assert_eq!(
        (
            states.disabled,
            states.focused,
            states.selected,
            states.expanded,
            states.required
        ),
        (Some(true), Some(true), Some(true), Some(false), Some(true))
    );
    assert_eq!(states.invalid.as_deref(), Some("spelling"));
    assert_eq!(
        (states.focusable, states.readonly, states.pressed),
        (None, None, None)
    );
    assert_eq!(node.properties, []);
    // A state attribute of another type fills nothing and stays a property.
    let other = child(&json!({ "AXEnabled": text("no") }));
    assert_eq!(other.states.disabled, None);
    assert_eq!(
        other.properties[0].value,
        PropertyValue::Text("no".to_owned())
    );
}

fn numbered(role: &str, value: &Value) -> AccessibleNode {
    child(&json!({ "AXRole": text(role), "AXValue": { "type": "number", "value": value } }))
}

#[test]
fn whole_number_values_fill_level_and_checked() {
    for value in [json!(2), json!(2.0)] {
        let heading = numbered("AXHeading", &value);
        assert_eq!(heading.states.level, Some(2), "{value}");
        assert!(
            heading
                .properties
                .iter()
                .all(|property| property.name != "AXValue")
        );
        assert_eq!(heading.value, None);
    }
    for (value, checked) in [
        (json!(0), Tristate::False),
        (json!(1.0), Tristate::True),
        (json!(2), Tristate::Mixed),
    ] {
        assert_eq!(numbered("AXCheckBox", &value).states.checked, Some(checked));
    }
    assert_eq!(
        numbered("AXRadioButton", &json!(1)).states.checked,
        Some(Tristate::True)
    );
    assert_eq!(
        numbered("AXRadioButton", &json!(0.0)).states.checked,
        Some(Tristate::False)
    );
}

#[test]
fn other_numbers_fill_nothing_and_stay_properties() {
    for (role, value) in [
        ("AXHeading", json!(2.5)),
        ("AXHeading", json!(0)),
        ("AXHeading", json!(10)),
        ("AXRadioButton", json!(2)),
        ("AXCheckBox", json!(3)),
        ("AXGroup", json!(1)),
    ] {
        let node = numbered(role, &value);
        assert_eq!(
            (node.states.level, node.states.checked),
            (None, None),
            "{role} {value}"
        );
        let property = node
            .properties
            .iter()
            .find(|property| property.name == "AXValue")
            .expect("kept");
        let expected = value.as_i64().map_or_else(
            || PropertyValue::Number(value.as_f64().expect("a number")),
            PropertyValue::Integer,
        );
        assert_eq!(property.value, expected, "{role} {value}");
    }
}

#[test]
fn properties_keep_scalars_in_name_order_and_drop_the_rest() {
    let node = child(&json!({
        "AXRole": text("AXGroup"),
        "AXSubrole": text("AXApplicationGroup"),
        "AXZed": { "type": "boolean", "value": true },
        "AXBlockQuoteLevel": { "type": "number", "value": 0 },
        "AXLoadingProgress": { "type": "number", "value": 0.5 },
        "AXURL": { "type": "url", "value": "http://127.0.0.1/" },
        "AXChildren": { "type": "array", "items": [] },
        "AXParent": { "type": "element", "node": 0 },
        "AXPosition": { "type": "point", "x": 1.0, "y": 2.0 },
        "AXSize": { "type": "size", "width": 1.0, "height": 2.0 },
        "AXFrame": { "type": "rect", "x": 1.0, "y": 2.0, "width": 3.0, "height": 4.0 },
        "AXVisibleCharacterRange": { "type": "range", "location": 0, "length": 1 },
        "_AXPrimaryScreenHeight": { "type": "number", "value": 900 },
        "AXLayoutCount": { "type": "number", "value": 4 },
        "AXStartTextMarker": { "type": "unsupported", "cf_type": "AXTextMarker" },
        "AXAccessKey": { "type": "error", "code": -25212 },
    }));
    let properties: Vec<(&str, &PropertyValue)> = node
        .properties
        .iter()
        .map(|property| (property.name.as_str(), &property.value))
        .collect();
    assert_eq!(
        properties,
        [
            ("AXBlockQuoteLevel", &PropertyValue::Integer(0)),
            ("AXLoadingProgress", &PropertyValue::Number(0.5)),
            ("AXRole", &PropertyValue::Text("AXGroup".to_owned())),
            (
                "AXSubrole",
                &PropertyValue::Text("AXApplicationGroup".to_owned())
            ),
            ("AXZed", &PropertyValue::Boolean(true)),
        ]
    );
}

#[test]
fn display_dependent_attributes_are_absent_from_every_capture() {
    for name in capture_names() {
        let model = adapted(name.to_str().expect("UTF-8"));
        for node in model.nodes.iter().filter_map(ExposedNode::as_accessible) {
            for property in &node.properties {
                assert!(
                    !super::DISPLAY_DEPENDENT.contains(&property.name.as_str()),
                    "{}",
                    name.display()
                );
            }
            assert!(
                node.properties
                    .iter()
                    .any(|property| property.name == "AXRole"),
                "{}",
                name.display()
            );
        }
    }
}

#[test]
fn diagnostics_are_the_snapshots() {
    let mut value = basic_value();
    value["diagnostics"] = json!([{ "code": "asset-not-found", "message": "a 404" }]);
    let model = adapt(&read_value(&value), &BTreeMap::new()).expect("adapts");
    assert_eq!(model.diagnostics.len(), 1);
    assert_eq!(
        (
            model.diagnostics[0].code.as_str(),
            model.diagnostics[0].dom.as_ref()
        ),
        ("asset-not-found", None)
    );
}

#[test]
fn long_page_adapts_well_under_a_second() {
    let name = Path::new("focus/long.json");
    let snapshot = read_value(&value_at(&captures_directory().join(name)));
    let lengths = source_lengths(name, &snapshot);
    let started = Instant::now();
    let model = adapt(&snapshot, &lengths).expect("focus/long.json adapts");
    let elapsed = started.elapsed();
    assert!(model.nodes.len() > 100);
    assert!(
        elapsed < Duration::from_secs(1),
        "adapting took {elapsed:?}"
    );
}
