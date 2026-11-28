use std::borrow::Cow;
use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use spokenpath_model::{
    AccessibleNode, Availability, ExposedNode, FocusStop, FocusWalk, Model, Role, chromium::adapt,
};
use spokenpath_snapshot::{Snapshot, read};

use crate::{
    AccessibleRecord, Approx, ENDING_WORDING, Ending, EndingKind, IndexEntry, Indexes, Item,
    LayerA, Marking, NO_ACCESSIBLE_NAME, NameSourceOutcome, NameStatus, NotAvailablePart, Options,
    RoleKind, Stop, TEMPLATES, Transcript, announce, escape_for_terminal,
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn goldens_directory() -> PathBuf {
    fixtures().join("snapshots/chromium")
}

/// Every capture under `fixtures/snapshots/chromium/`, relative to it, found by listing the
/// directories, so a new capture without a render golden fails the golden test.
fn golden_names() -> Vec<PathBuf> {
    let root = goldens_directory();
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
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            {
                let relative = path.strip_prefix(&root).expect("listed under the root");
                names.push(relative.to_path_buf());
            }
        }
    }
    names.sort();
    names
}

fn golden_value(name: &Path) -> Value {
    let path = goldens_directory().join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is JSON: {error}", path.display()))
}

/// The byte length of each source of a golden, read from its page under `fixtures/pages/` at the
/// golden's relative directory. Each path is canonicalised and must stay under `fixtures/pages/`
/// before it is read, because the source path comes from the snapshot.
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
                .unwrap_or_else(|error| panic!("{} has its page: {error}", name.display()));
            assert!(
                page.starts_with(&pages),
                "{} names a source outside fixtures/pages: {}",
                name.display(),
                page.display()
            );
            let length = std::fs::metadata(&page)
                .unwrap_or_else(|error| panic!("{} is readable: {error}", page.display()))
                .len();
            (source.id, length)
        })
        .collect()
}

fn read_value(name: &Path, value: &Value) -> Snapshot {
    read(&value.to_string()).unwrap_or_else(|error| panic!("{} reads: {error}", name.display()))
}

fn adapted_path(name: &Path) -> Model {
    let snapshot = read_value(name, &golden_value(name));
    adapt(&snapshot, &source_lengths(name, &snapshot))
        .unwrap_or_else(|error| panic!("{} adapts: {error}", name.display()))
}

fn adapted(name: &str) -> Model {
    adapted_path(Path::new(name))
}

fn with_layer_b(model: &Model) -> Transcript {
    announce(model, Options::default())
}

fn without_layer_b(model: &Model) -> Transcript {
    announce(model, Options { layer_b: false })
}

fn to_value(transcript: &Transcript) -> Value {
    serde_json::to_value(transcript).expect("a transcript serialises")
}

/// The insta snapshot name of a capture: its relative path without the extension, with `__`
/// between directories, the same on every platform.
fn snapshot_name(name: &Path) -> String {
    name.with_extension("")
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("__")
}

#[test]
fn every_capture_matches_its_render_golden() {
    let names = golden_names();
    for directory in ["focus", "names", "reading-order", "navigation"] {
        assert!(
            names.iter().any(|name| name.starts_with(directory)),
            "no capture under {directory}/"
        );
    }
    for name in &names {
        let transcript = with_layer_b(&adapted_path(name));
        let json = serde_json::to_string_pretty(&transcript).expect("a transcript serialises");
        insta::assert_snapshot!(snapshot_name(name), json);
    }
}

/// The item of the element whose `id` attribute is `id`.
fn item<'t>(transcript: &'t Transcript, id: &str) -> &'t Item {
    transcript
        .nodes
        .iter()
        .find(|item| {
            accessible(item)
                .and_then(|record| record.dom.as_ref())
                .is_some_and(|dom| dom.id.as_deref() == Some(id))
        })
        .unwrap_or_else(|| panic!("an item with id {id:?}"))
}

fn accessible(item: &Item) -> Option<&AccessibleRecord> {
    match &item.layer_a {
        LayerA::Accessible(record) => Some(record),
        LayerA::TextRun(_) => None,
    }
}

fn numbers(transcript: &Transcript, ids: &[&str]) -> Vec<usize> {
    ids.iter().map(|id| item(transcript, id).number).collect()
}

fn dom_id(transcript: &Transcript, id: &str) -> u32 {
    accessible(item(transcript, id))
        .and_then(|record| record.dom.as_ref())
        .map(|dom| dom.dom_id)
        .expect("a DOM node")
}

/// The excluded nodes of a transcript whose engine supplies them.
fn excluded_of(transcript: &Transcript) -> &[crate::Excluded] {
    transcript.excluded.as_deref().expect("excluded nodes")
}

/// The tab order of a transcript whose engine supplies it.
fn tab_order_of(transcript: &Transcript) -> &crate::TabOrder {
    transcript.tab_order.as_ref().expect("a tab order")
}

/// The focus walk of a model whose engine supplies it.
fn focus_of(model: &Model) -> &FocusWalk {
    model.focus.available().expect("a focus walk")
}

fn is_increasing(numbers: &[usize]) -> bool {
    numbers.windows(2).all(|pair| pair[0] < pair[1])
}

#[test]
fn items_are_numbered_in_pre_order_with_depth_and_parent() {
    for name in golden_names() {
        let model = adapted_path(&name);
        let transcript = without_layer_b(&model);
        assert_eq!(
            transcript.nodes.len(),
            model.nodes.len(),
            "{}",
            name.display()
        );
        let mut depths = Vec::new();
        for (position, item) in transcript.nodes.iter().enumerate() {
            assert_eq!(item.number, position + 1, "{}", name.display());
            if let Some(parent) = item.parent {
                assert!(parent < item.number, "{}", name.display());
                assert_eq!(item.depth, depths[parent - 1] + 1, "{}", name.display());
            } else {
                assert_eq!((item.number, item.depth), (1, 0), "{}", name.display());
            }
            depths.push(item.depth);
        }
    }
}

#[test]
fn shadow_slots_follow_the_flat_tree_order_not_the_dom_order() {
    let transcript = with_layer_b(&adapted("reading-order/shadow-slots.json"));
    let host = item(&transcript, "host").number;
    let flat = [
        "shadow-heading",
        "light-titled",
        "shadow-paragraph",
        "light-default-a",
        "light-default-b",
    ];
    let order = numbers(&transcript, &flat);
    assert!(is_increasing(&order), "{order:?}");
    for id in flat {
        assert_eq!(item(&transcript, id).parent, Some(host), "{id}");
    }
    // In the DOM the titled button sits between the two default-slot buttons.
    assert!(dom_id(&transcript, "light-default-a") < dom_id(&transcript, "light-titled"));
    assert!(dom_id(&transcript, "light-titled") < dom_id(&transcript, "light-default-b"));
}

#[test]
fn aria_owns_moves_the_owned_item_into_the_owning_list() {
    let transcript = with_layer_b(&adapted("reading-order/aria-owns.json"));
    let list = item(&transcript, "owner").number;
    let owned = item(&transcript, "owned");
    assert_eq!(owned.parent, Some(list));
    assert_eq!(item(&transcript, "dom-first").parent, Some(list));
    assert_eq!(item(&transcript, "dom-second").parent, Some(list));
    assert!(is_increasing(&numbers(
        &transcript,
        &["dom-first", "dom-second", "owned"]
    )));
    let original_parent = item(&transcript, "original-parent").number;
    assert_ne!(owned.parent, Some(original_parent));
    assert_eq!(
        item(&transcript, "left-behind").parent,
        Some(original_parent)
    );
}

#[test]
fn reading_flow_reorders_only_the_groups_that_set_it() {
    let transcript = with_layer_b(&adapted("reading-order/reading-flow.json"));
    for (group, visual) in [
        (
            "flex-reading-flow",
            ["flex-flow-2", "flex-flow-3", "flex-flow-1"],
        ),
        (
            "grid-reading-flow",
            ["grid-flow-2", "grid-flow-3", "grid-flow-1"],
        ),
        (
            "flex-order-only",
            ["flex-order-1", "flex-order-2", "flex-order-3"],
        ),
        (
            "grid-placement-only",
            ["grid-place-1", "grid-place-2", "grid-place-3"],
        ),
    ] {
        let parent = item(&transcript, group).number;
        let children: Vec<usize> = transcript
            .nodes
            .iter()
            .filter(|item| item.parent == Some(parent))
            .map(|item| item.number)
            .collect();
        assert_eq!(children, numbers(&transcript, &visual), "{group}");
    }
}

fn entry(item: usize, role: &str, name: &str) -> IndexEntry {
    IndexEntry {
        item,
        role: role.to_owned(),
        name_status: NameStatus::Named,
        name: Some(name.to_owned()),
    }
}

fn unnamed_entry(item: usize, role: &str) -> IndexEntry {
    IndexEntry {
        item,
        role: role.to_owned(),
        name_status: NameStatus::NoAccessibleName,
        name: None,
    }
}

#[test]
fn landmarks_page_lists_headings_landmarks_links_and_form_controls() {
    let transcript = with_layer_b(&adapted("navigation/landmarks.json"));
    let number = |id| item(&transcript, id).number;
    let indexes = &transcript.indexes;

    let headings: Vec<(usize, Option<u32>, Option<&str>)> = indexes
        .headings
        .iter()
        .map(|heading| {
            (
                heading.entry.item,
                heading.level,
                heading.entry.name.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        headings,
        [
            (number("heading-1"), Some(1), Some("Page title")),
            (number("heading-2"), Some(2), Some("Section heading")),
            (number("heading-3"), Some(3), Some("Subsection heading")),
        ]
    );
    assert_eq!(
        indexes.landmarks,
        [
            unnamed_entry(number("banner"), "banner"),
            entry(number("site-nav"), "navigation", "Site"),
            unnamed_entry(number("main"), "main"),
            entry(number("region"), "region", "Featured"),
            entry(number("contact"), "form", "Contact"),
            unnamed_entry(number("site-search"), "search"),
            unnamed_entry(number("aside"), "complementary"),
            unnamed_entry(number("footer"), "contentinfo"),
        ]
    );
    assert_eq!(
        indexes.links,
        [
            entry(number("link-home"), "link", "Home"),
            entry(number("link-about"), "link", "About"),
        ]
    );
    assert_eq!(
        indexes.form_controls,
        [
            entry(number("full-name"), "textbox", "Full name"),
            entry(number("subscribe"), "checkbox", "Subscribe"),
            entry(number("plan-basic"), "radio", "Basic"),
            entry(number("plan-pro"), "radio", "Pro"),
            entry(number("country"), "combobox", "Country"),
            entry(number("message"), "textbox", "Message"),
            entry(number("send"), "button", "Send"),
        ]
    );
}

#[test]
fn index_membership_ignores_internal_roles_and_tags() {
    let transcript = with_layer_b(&adapted("navigation/landmarks.json"));
    let listed: Vec<usize> = [
        transcript.indexes.landmarks.as_slice(),
        &transcript.indexes.links,
        &transcript.indexes.form_controls,
    ]
    .concat()
    .iter()
    .map(|entry| entry.item)
    .chain(
        transcript
            .indexes
            .headings
            .iter()
            .map(|heading| heading.entry.item),
    )
    .collect();
    for item in &transcript.nodes {
        let role = accessible(item).and_then(|record| record.role.as_ref());
        if role.is_none_or(|role| role.kind == RoleKind::Internal) {
            assert!(!listed.contains(&item.number), "item {}", item.number);
        }
    }
}

#[test]
fn unnamed_icon_button_says_so_and_shows_every_source_tried() {
    let transcript = with_layer_b(&adapted("names/icon-button.json"));
    let button = item(&transcript, "target");
    let record = accessible(button).expect("an accessible item");
    assert_eq!(record.name.status, NameStatus::NoAccessibleName);
    let tried: Vec<(&str, Option<&str>, Option<&str>, NameSourceOutcome)> = record
        .name
        .sources
        .iter()
        .flatten()
        .map(|source| {
            (
                source.kind.as_str(),
                source.attribute.as_deref(),
                source.native_source.as_deref(),
                source.outcome,
            )
        })
        .collect();
    assert_eq!(
        tried,
        [
            (
                "related-element",
                Some("aria-labelledby"),
                None,
                NameSourceOutcome::Empty
            ),
            (
                "attribute",
                Some("aria-label"),
                None,
                NameSourceOutcome::Empty
            ),
            (
                "related-element",
                None,
                Some("label"),
                NameSourceOutcome::Empty
            ),
            ("contents", None, None, NameSourceOutcome::Empty),
            ("attribute", Some("title"), None, NameSourceOutcome::Empty),
        ]
    );
    assert_eq!(
        button.approx,
        Some(Approx {
            template: "unnamed",
            phrase: "button".to_owned(),
        })
    );
    let json = to_value(&transcript);
    let name = &json["nodes"][button.number - 1]["layer_a"]["name"];
    assert_eq!(name["status"], "no accessible name");

    let excluded: Vec<(Option<&str>, Vec<&str>)> = excluded_of(&transcript)
        .iter()
        .filter(|node| matches!(node.dom.tag.as_deref(), Some("svg" | "path")))
        .map(|node| {
            (
                node.dom.tag.as_deref(),
                node.reasons.iter().map(|reason| reason.kind).collect(),
            )
        })
        .collect();
    assert_eq!(
        excluded,
        [
            (Some("svg"), vec!["aria-hidden"]),
            (Some("path"), vec!["aria-hidden"])
        ]
    );
    let path = excluded_of(&transcript)
        .iter()
        .find(|node| node.dom.tag.as_deref() == Some("path"))
        .expect("the path is excluded");
    let ancestor = path.reasons[0]
        .hiding_ancestor
        .as_ref()
        .expect("a hiding ancestor");
    assert_eq!(ancestor.value.dom.id.as_deref(), Some("icon"));
    assert_eq!(ancestor.by, Marking::ExposedByEngine);
}

#[test]
fn a_name_absent_from_the_model_says_so_with_an_empty_list_of_sources() {
    let mut model = adapted("names/icon-button.json");
    let target = model
        .nodes
        .iter_mut()
        .filter_map(|node| match node {
            ExposedNode::Accessible(node) => Some(node),
            ExposedNode::TextRun(_) => None,
        })
        .find(|node| {
            node.dom
                .as_ref()
                .is_some_and(|dom| dom.id.as_deref() == Some("target"))
        })
        .expect("the button");
    target.name = None;
    let transcript = with_layer_b(&model);
    let json = to_value(&transcript);
    let number = item(&transcript, "target").number;
    assert_eq!(
        json["nodes"][number - 1]["layer_a"]["name"],
        serde_json::json!({"status": "no accessible name", "sources": []})
    );
}

/// Every phrase of a transcript, with the Layer A strings of its item that it may equal: a text
/// run's text, a name alone, or a role token alone. The role token is exempt because an unnamed
/// item with no parts has the bare role as its required phrase, as `button` for an icon button.
fn phrases(transcript: &Transcript) -> Vec<(&str, Vec<&str>)> {
    transcript
        .nodes
        .iter()
        .filter_map(|item| {
            let approx = item.approx.as_ref()?;
            let own = match &item.layer_a {
                LayerA::TextRun(run) => vec![run.text.as_str()],
                LayerA::Accessible(record) => record
                    .name
                    .text
                    .as_deref()
                    .into_iter()
                    .chain(record.role.as_ref().map(|role| role.token.as_str()))
                    .collect(),
            };
            Some((approx.phrase.as_str(), own))
        })
        .collect()
}

/// Every string in `value`, except the `phrase` of an `approx` object.
fn strings_outside_phrases<'v>(value: &'v Value, under_approx: bool, strings: &mut Vec<&'v str>) {
    let mut pending = vec![(value, under_approx)];
    while let Some((value, under_approx)) = pending.pop() {
        match value {
            Value::String(text) => strings.push(text),
            Value::Array(values) => pending.extend(values.iter().map(|value| (value, false))),
            Value::Object(fields) => {
                for (key, value) in fields {
                    if !(under_approx && key == "phrase") {
                        pending.push((value, key == "approx"));
                    }
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }
}

/// Removes every `approx` key from `value`.
fn without_approx(value: &mut Value) {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        match value {
            Value::Array(values) => pending.extend(values.iter_mut()),
            Value::Object(fields) => {
                fields.remove("approx");
                pending.extend(fields.values_mut());
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }
}

/// Every key of every object in `value`, with the object's key in its parent.
fn keys(value: &Value) -> Vec<(String, Option<String>)> {
    let mut keys = Vec::new();
    let mut pending = vec![(value, None::<&str>)];
    while let Some((value, parent)) = pending.pop() {
        match value {
            Value::Array(values) => pending.extend(values.iter().map(|value| (value, parent))),
            Value::Object(fields) => {
                for (key, value) in fields {
                    keys.push((key.clone(), parent.map(str::to_owned)));
                    pending.push((value, Some(key)));
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }
    keys
}

#[test]
fn layer_b_stays_under_approx_in_every_capture() {
    let identifiers: Vec<&str> = TEMPLATES.iter().map(|template| template.id).collect();
    for name in golden_names() {
        let label = name.display();
        let model = adapted_path(&name);
        let transcript = with_layer_b(&model);
        let mut json = to_value(&transcript);

        for (index, item) in json["nodes"].as_array().expect("nodes").iter().enumerate() {
            let approx = item["approx"].as_object().expect("every item has approx");
            let mut approx_keys: Vec<&str> = approx.keys().map(String::as_str).collect();
            approx_keys.sort_unstable();
            assert_eq!(
                approx_keys,
                ["phrase", "template"],
                "{label}: item {}",
                index + 1
            );
            let template = approx["template"].as_str().expect("a template identifier");
            assert!(identifiers.contains(&template), "{label}: {template}");
        }
        for (key, parent) in keys(&json) {
            if key == "approx" {
                assert_eq!(
                    parent.as_deref(),
                    Some("nodes"),
                    "{label}: approx outside an item"
                );
            }
        }

        let mut strings = Vec::new();
        strings_outside_phrases(&json, false, &mut strings);
        let strings: HashSet<&str> = strings.into_iter().collect();
        for (phrase, own) in phrases(&transcript) {
            if phrase.is_empty() || own.contains(&phrase) {
                continue;
            }
            assert!(
                !strings.contains(phrase),
                "{label}: the phrase {phrase:?} appears outside approx"
            );
        }

        without_approx(&mut json);
        assert_eq!(json, to_value(&without_layer_b(&model)), "{label}");
    }
}

#[test]
fn without_layer_b_no_capture_has_approx_content() {
    let identifiers: Vec<&str> = TEMPLATES.iter().map(|template| template.id).collect();
    for name in golden_names() {
        let label = name.display();
        let transcript = without_layer_b(&adapted_path(&name));
        assert!(
            transcript.nodes.iter().all(|item| item.approx.is_none()),
            "{label}"
        );
        let json = to_value(&transcript);
        for (key, _) in keys(&json) {
            assert!(key != "approx" && key != "template", "{label}: key {key}");
        }
        let text = serde_json::to_string(&json).expect("serialises");
        assert!(!text.contains("\"approx\""), "{label}");
        // Two identifiers are also Layer A words: the `named` name status and the `text-run`
        // record kind. Every other identifier never appears.
        for &identifier in &identifiers {
            if !matches!(identifier, "named" | "text-run") {
                assert!(
                    !text.contains(&format!("\"{identifier}\"")),
                    "{label}: {identifier}"
                );
            }
        }
    }
}

#[test]
fn unnamed_phrases_hold_no_name_text_or_tried_values() {
    for name in golden_names() {
        let transcript = with_layer_b(&adapted_path(&name));
        for item in &transcript.nodes {
            let (Some(record), Some(approx)) = (accessible(item), &item.approx) else {
                continue;
            };
            if record.name.status != NameStatus::NoAccessibleName {
                continue;
            }
            assert!(
                matches!(approx.template, "unnamed" | "unnamed-unspoken-role"),
                "{}: item {}",
                name.display(),
                item.number
            );
            let tried =
                record
                    .name
                    .text
                    .iter()
                    .chain(record.name.sources.iter().flatten().flat_map(|source| {
                        source.attribute_value.iter().chain(source.value.iter())
                    }))
                    .filter(|text| !text.trim().is_empty());
            for text in tried {
                assert!(
                    !approx.phrase.contains(text.as_str()),
                    "{}: item {} phrase {:?} holds {text:?}",
                    name.display(),
                    item.number,
                    approx.phrase
                );
            }
        }
    }
}

#[test]
fn phrases_follow_the_template_table() {
    let transcript = with_layer_b(&adapted("navigation/landmarks.json"));
    let phrase = |id| {
        let approx = item(&transcript, id).approx.as_ref().expect("approx");
        (approx.template, approx.phrase.as_str())
    };
    assert_eq!(
        phrase("heading-2"),
        ("named", "Section heading, heading, level 2")
    );
    assert_eq!(phrase("banner"), ("unnamed", "banner"));
    assert_eq!(
        phrase("subscribe"),
        ("named", "Subscribe, checkbox, not checked")
    );
    assert_eq!(
        phrase("plan-pro"),
        ("named", "Pro, radio, not checked, 2 of 2")
    );
    assert_eq!(
        phrase("country"),
        ("named", "Country, combobox, Canada, collapsed")
    );
    let root = transcript.nodes[0].approx.as_ref().expect("approx");
    assert_eq!(
        (root.template, root.phrase.as_str()),
        ("named-unspoken-role", "Landmarks and headings")
    );
    let generic = transcript
        .nodes
        .iter()
        .find(|item| {
            accessible(item)
                .and_then(|record| record.role.as_ref())
                .is_some_and(|role| role.token == "generic")
        })
        .and_then(|item| item.approx.as_ref())
        .expect("a generic item");
    assert_eq!(
        (generic.template, generic.phrase.as_str()),
        ("unnamed-unspoken-role", "")
    );
    let run = transcript
        .nodes
        .iter()
        .find(|item| matches!(&item.layer_a, LayerA::TextRun(run) if run.text == "Footer text"))
        .and_then(|item| item.approx.as_ref())
        .expect("a text run");
    assert_eq!(
        (run.template, run.phrase.as_str()),
        ("text-run", "Footer text")
    );
}

fn ending(name: &str) -> (Ending, Marking) {
    let transcript = with_layer_b(&adapted(name));
    let tab_order = tab_order_of(&transcript);
    (tab_order.ending.value.clone(), tab_order.ending.by)
}

fn repeats(name: &str) -> Vec<(usize, Option<usize>)> {
    with_layer_b(&adapted(name))
        .tab_order
        .expect("a tab order")
        .steps
        .iter()
        .map(|step| (step.step, step.repeats))
        .collect()
}

#[test]
fn tab_order_endings_match_the_focus_captures() {
    let cases = [
        (
            "sign-in.json",
            EndingKind::Wrap,
            "Focus left the page and Tab returned to step 1, the normal end of a walk.",
        ),
        (
            "focus/trap.json",
            EndingKind::Trap,
            "Tab returned to step 1 without focus leaving the page: a focus trap.",
        ),
        (
            "focus/self-trap.json",
            EndingKind::Trap,
            "Tab returned to step 2 without focus leaving the page: a focus trap.",
        ),
        (
            "focus/long.json",
            EndingKind::Truncated,
            "The walk stopped at its limit after 200 Tab presses. Later stops are not listed.",
        ),
        (
            "focus/iframe.json",
            EndingKind::Wrap,
            "Focus left the page and Tab returned to step 1, the normal end of a walk.",
        ),
        (
            "focus/no-focusable.json",
            EndingKind::LeftPage,
            "Focus left the page without stopping on any element.",
        ),
    ];
    for (name, kind, wording) in cases {
        let (ending, by) = ending(name);
        assert_eq!(
            (ending.kind, ending.wording.as_str()),
            (kind, wording),
            "{name}"
        );
        assert_eq!(by, Marking::ComputedByTool, "{name}");
    }
}

#[test]
fn the_repeating_step_names_the_step_it_repeats() {
    let sign_in = with_layer_b(&adapted("sign-in.json"));
    let kinds: Vec<&Stop> = tab_order_of(&sign_in)
        .steps
        .iter()
        .map(|step| &step.stop)
        .collect();
    assert!(matches!(kinds[3], Stop::LeftPage));
    let Stop::Element {
        item: Some(first), ..
    } = kinds[0]
    else {
        panic!("step 1 is an exposed element");
    };
    let Stop::Element {
        item: Some(fifth), ..
    } = kinds[4]
    else {
        panic!("step 5 is an exposed element");
    };
    assert_eq!(first, fifth);
    assert_eq!(
        repeats("sign-in.json"),
        [(1, None), (2, None), (3, None), (4, None), (5, Some(1))]
    );
    assert_eq!(
        repeats("focus/trap.json"),
        [(1, None), (2, None), (3, None), (4, Some(1))]
    );
    assert_eq!(
        repeats("focus/self-trap.json"),
        [(1, None), (2, None), (3, Some(2))]
    );
    // Steps 2 and 3 focus the same frame element one after the other, which is not a cycle.
    assert_eq!(
        repeats("focus/iframe.json"),
        [
            (1, None),
            (2, None),
            (3, None),
            (4, None),
            (5, None),
            (6, Some(1))
        ]
    );
    let no_focusable = with_layer_b(&adapted("focus/no-focusable.json"));
    assert!(
        no_focusable
            .tab_order
            .as_ref()
            .expect("a tab order")
            .steps
            .iter()
            .all(|step| step.stop == Stop::LeftPage && step.repeats.is_none())
    );
    assert_eq!(tab_order_of(&no_focusable).steps.len(), 2);
    assert_eq!(
        with_layer_b(&adapted("focus/long.json"))
            .tab_order
            .as_ref()
            .expect("a tab order")
            .steps
            .len(),
        200
    );
}

#[test]
fn tool_computed_values_read_computed_by_tool_and_engine_ones_exposed_by_engine() {
    let list = to_value(&with_layer_b(&adapted("set-position/list.json")));
    let first = list["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|item| item["layer_a"]["dom"]["id"] == "first")
        .expect("the first list item");
    assert_eq!(
        first["layer_a"]["set_position"],
        serde_json::json!({"value": {"position": 1, "size": 3}, "by": "computed by tool"})
    );
    assert_eq!(first["layer_a"]["role_origin"]["by"], "computed by tool");

    let described = to_value(&with_layer_b(&adapted(
        "names/description-describedby.json",
    )));
    let target = described["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|item| item["layer_a"]["dom"]["id"] == "target")
        .expect("the described button");
    let provenance = &target["layer_a"]["description"]["provenance"];
    assert_eq!(provenance["by"], "computed by tool");
    assert_eq!(provenance["value"]["attribute"], "aria-describedby");

    let icon = to_value(&with_layer_b(&adapted("names/icon-button.json")));
    let ancestors: Vec<&Value> = icon["excluded"]
        .as_array()
        .expect("excluded")
        .iter()
        .flat_map(|node| node["reasons"].as_array().expect("reasons"))
        .map(|reason| &reason["hiding_ancestor"])
        .filter(|ancestor| !ancestor.is_null())
        .collect();
    assert!(
        ancestors
            .iter()
            .any(|ancestor| ancestor["by"] == "exposed by engine")
    );
}

#[test]
fn only_supplied_values_carry_a_marking() {
    // Every object holding a `by` key is a marked value: exactly `value` and `by`.
    for name in golden_names() {
        let json = to_value(&with_layer_b(&adapted_path(&name)));
        let mut pending = vec![&json];
        while let Some(value) = pending.pop() {
            match value {
                Value::Array(values) => pending.extend(values),
                Value::Object(fields) => {
                    if let Some(by) = fields.get("by") {
                        assert!(
                            by == "computed by tool" || by == "exposed by engine",
                            "{}: {by}",
                            name.display()
                        );
                        let mut keys: Vec<&str> = fields.keys().map(String::as_str).collect();
                        keys.sort_unstable();
                        assert_eq!(keys, ["by", "value"], "{}", name.display());
                    }
                    pending.extend(fields.values());
                }
                Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
            }
        }
    }
}

#[test]
fn escape_for_terminal_escapes_only_characters_that_disturb_a_terminal() {
    for clean in ["Café 日本語", "Ünïcødé ñ 한국어 العربية"] {
        assert!(
            matches!(escape_for_terminal(clean), Cow::Borrowed(text) if text == clean),
            "{clean:?}"
        );
    }
    assert_eq!(
        escape_for_terminal("\u{1b}[31mred\u{7}\u{0}\u{7f}\u{9b}2J\u{202e}x\u{2066}y\u{2069}\n"),
        "\\u{1b}[31mred\\u{7}\\u{0}\\u{7f}\\u{9b}2J\\u{202e}x\\u{2066}y\\u{2069}\\u{a}"
    );
    let cases = [
        // Bidirectional embedding, override and isolate controls.
        ('\u{202a}', "\\u{202a}"),
        ('\u{202e}', "\\u{202e}"),
        ('\u{2066}', "\\u{2066}"),
        ('\u{2069}', "\\u{2069}"),
        // Implicit direction marks.
        ('\u{200e}', "\\u{200e}"),
        ('\u{200f}', "\\u{200f}"),
        ('\u{61c}', "\\u{61c}"),
        // Line and paragraph separators.
        ('\u{2028}', "\\u{2028}"),
        ('\u{2029}', "\\u{2029}"),
        // Zero-width characters.
        ('\u{200b}', "\\u{200b}"),
        ('\u{200c}', "\\u{200c}"),
        ('\u{200d}', "\\u{200d}"),
        ('\u{2060}', "\\u{2060}"),
        ('\u{feff}', "\\u{feff}"),
        // Invisible operators.
        ('\u{2061}', "\\u{2061}"),
        ('\u{2062}', "\\u{2062}"),
        ('\u{2064}', "\\u{2064}"),
        // Unicode tag characters.
        ('\u{e0000}', "\\u{e0000}"),
        ('\u{e0041}', "\\u{e0041}"),
        ('\u{e007f}', "\\u{e007f}"),
    ];
    for (character, escaped) in cases {
        assert_eq!(
            escape_for_terminal(&format!("a{character}b")),
            format!("a{escaped}b"),
            "{character:?}"
        );
    }
    // The neighbours of each escaped range are ordinary characters.
    let neighbours = "\u{61b}\u{61d}\u{200a}\u{2010}\u{2027}\u{202f}\u{205f}\u{2065}\u{206a}\u{fefe}\u{ff00}\u{dffff}\u{e0080}";
    assert!(matches!(escape_for_terminal(neighbours), Cow::Borrowed(_)));
}

#[test]
fn escape_for_terminal_keeps_variation_selectors_so_emoji_keep_their_presentation() {
    for kept in [
        "\u{2764}\u{fe0f}",
        "\u{263a}\u{fe0e}",
        "\u{fe00}\u{fe0f}",
        "\u{845b}\u{e0100}\u{e01ef}",
    ] {
        assert!(
            matches!(escape_for_terminal(kept), Cow::Borrowed(text) if text == kept),
            "{kept:?}"
        );
    }
    // A subdivision flag is a black flag, tag letters and a cancel tag; the tags are escaped.
    assert_eq!(
        escape_for_terminal("\u{1f3f4}\u{e0067}\u{e0062}\u{e007f}"),
        "\u{1f3f4}\\u{e0067}\\u{e0062}\\u{e007f}"
    );
}

fn is_unsafe_for_a_terminal(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{202A}'..='\u{202E}'
                | '\u{2066}'..='\u{2069}'
                | '\u{200B}'..='\u{200F}'
                | '\u{061C}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{2060}'..='\u{2064}'
                | '\u{FEFF}'
                | '\u{E0000}'..='\u{E007F}'
        )
}

#[test]
fn phrases_escape_page_text_and_layer_a_keeps_it_raw() {
    const NAME: &str = "\u{1b}[31mCafé\u{7}";
    const TEXT: &str = "日本語\u{0}\u{9b}31m\u{202e}txet";
    const VALUE: &str = "you@example.com\u{1b}[0m";
    let mut model = adapted("sign-in.json");
    for node in &mut model.nodes {
        match node {
            ExposedNode::Accessible(node) => {
                let role = node.role.as_ref().and_then(Role::aria);
                if role == Some("heading") {
                    node.name.as_mut().expect("the heading is named").text = NAME.to_owned();
                } else if role == Some("textbox") {
                    node.value = Some(VALUE.to_owned());
                }
            }
            ExposedNode::TextRun(run) => run.text = TEXT.to_owned(),
        }
    }
    let transcript = with_layer_b(&model);
    for item in &transcript.nodes {
        let phrase = &item.approx.as_ref().expect("approx").phrase;
        assert!(!phrase.chars().any(is_unsafe_for_a_terminal), "{phrase:?}");
    }
    let phrases: Vec<&str> = transcript
        .nodes
        .iter()
        .map(|item| item.approx.as_ref().expect("approx").phrase.as_str())
        .collect();
    assert!(
        phrases.contains(&"\\u{1b}[31mCafé\\u{7}, heading, level 1"),
        "{phrases:?}"
    );
    assert!(
        phrases.contains(&"日本語\\u{0}\\u{9b}31m\\u{202e}txet"),
        "{phrases:?}"
    );
    assert!(
        phrases
            .iter()
            .any(|phrase| phrase.contains("you@example.com\\u{1b}[0m")),
        "{phrases:?}"
    );

    let json = to_value(&transcript);
    let mut strings = Vec::new();
    strings_outside_phrases(&json, false, &mut strings);
    for raw in [NAME, TEXT, VALUE] {
        assert!(strings.contains(&raw), "Layer A keeps {raw:?} raw");
    }
}

#[test]
fn deep_chain_announces_without_overflowing_the_stack() {
    const DEPTH: u32 = 100_000;
    let mut value = golden_value(Path::new("sign-in.json"));
    value["spans"] = json!([]);
    value["diagnostics"] = json!([]);
    let mut dom = vec![json!({
        "id": 0, "backend_id": 1, "children": [1], "kind": "document", "attributes": []
    })];
    let mut ax = vec![json!({
        "nodeId": "0", "ignored": false, "role": {"type": "internalRole", "value": "RootWebArea"},
        "childIds": ["1"], "backendDOMNodeId": 1
    })];
    for id in 1..=DEPTH {
        let last = id == DEPTH;
        dom.push(json!({
            "id": id, "backend_id": id + 1, "parent": id - 1,
            "children": if last { json!([]) } else { json!([id + 1]) },
            "kind": "element", "tag": "div", "attributes": [], "origin": "script"
        }));
        ax.push(json!({
            "nodeId": id.to_string(), "ignored": false, "role": {"type": "role", "value": "generic"},
            "parentId": (id - 1).to_string(),
            "childIds": if last { json!([]) } else { json!([(id + 1).to_string()]) },
            "backendDOMNodeId": id + 1
        }));
    }
    let payload = value["payload"].as_object_mut().expect("a payload");
    payload.insert("dom".to_owned(), Value::Array(dom));
    payload.insert("ax".to_owned(), Value::Array(ax));
    payload.insert("ax_forced".to_owned(), json!([]));
    payload.insert("focus".to_owned(), json!({"steps": [], "truncated": false}));
    payload.remove("dom_snapshot");
    let snapshot = read_value(Path::new("chain"), &value);
    let model = adapt(&snapshot, &BTreeMap::new()).expect("the chain adapts");

    let transcript = with_layer_b(&model);
    let depth = usize::try_from(DEPTH).expect("fits");
    assert_eq!(transcript.nodes.len(), depth + 1);
    let deepest = transcript.nodes.last().expect("an item");
    assert_eq!((deepest.depth, deepest.parent), (depth, Some(depth)));
    let json = serde_json::to_string(&transcript).expect("a deep transcript serialises");
    assert!(json.len() > depth);
}

#[test]
fn long_page_announces_well_under_a_second() {
    let model = adapted("focus/long.json");
    let started = Instant::now();
    let transcript = with_layer_b(&model);
    let elapsed = started.elapsed();
    assert!(transcript.nodes.len() > 100);
    assert!(
        elapsed < Duration::from_secs(1),
        "announcing took {elapsed:?}"
    );
}

/// `model` reduced to its root, with no children, and an empty focus walk.
fn root_only(mut model: Model) -> Model {
    assert_eq!(model.root.get(), 0, "the adapter puts the root first");
    model.nodes.truncate(1);
    if let Some(ExposedNode::Accessible(root)) = model.nodes.first_mut() {
        root.children.clear();
    }
    model.focus = Availability::Available(FocusWalk {
        steps: Vec::new(),
        truncated: false,
        cycle_start: None,
    });
    model
}

#[test]
fn a_root_alone_with_no_focus_steps_has_one_item_and_empty_indexes() {
    let model = root_only(adapted("sign-in.json"));
    let transcript = with_layer_b(&model);
    assert_eq!(transcript.nodes.len(), 1);
    assert_eq!(transcript.indexes, Indexes::default());
    assert_eq!(
        excluded_of(&transcript).len(),
        model.excluded.available().map_or(0, Vec::len)
    );
    assert_eq!(tab_order_of(&transcript).steps, []);
    assert_eq!(
        tab_order_of(&transcript).ending.value.kind,
        EndingKind::Ended
    );
    assert_eq!(
        tab_order_of(&transcript).ending.value.wording,
        "The walk ended after 0 steps."
    );
    let json = to_value(&transcript);
    assert_eq!(
        json["indexes"],
        json!({"headings": [], "landmarks": [], "links": [], "form_controls": []})
    );
    let mut top: Vec<&str> = json
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    top.sort_unstable();
    assert_eq!(top, ["excluded", "header", "indexes", "nodes", "tab_order"]);
    assert_eq!(
        json["header"],
        json!({
            "engine": "chromium",
            "engine_version": model.engine_version,
            "tool_version": model.tool_version,
        })
    );
}

#[test]
fn node_indexes_from_another_model_are_left_absent() {
    let long = adapted("focus/long.json");
    let mut model = root_only(adapted("sign-in.json"));
    model.focus = long.focus.clone();
    let transcript = with_layer_b(&model);
    assert_eq!(tab_order_of(&transcript).steps.len(), 200);
    assert!(
        transcript
            .tab_order
            .as_ref()
            .expect("a tab order")
            .steps
            .iter()
            .all(|step| match &step.stop {
                Stop::Element { item, .. } => item.is_none(),
                Stop::LeftPage | Stop::Unresolved { .. } => true,
            })
    );

    let mut model = root_only(adapted("sign-in.json"));
    model.root = focus_of(&long)
        .steps
        .iter()
        .find_map(|stop| match stop {
            FocusStop::Element { node, .. } => *node,
            FocusStop::LeftPage | FocusStop::Unresolved { .. } => None,
        })
        .expect("an exposed stop");
    assert_eq!(with_layer_b(&model).nodes, []);
}

#[test]
fn user_facing_wording_makes_no_claims() {
    const FORBIDDEN: [&str; 9] = [
        "wcag",
        "conform",
        "voiceover",
        "nvda",
        "jaws",
        "talkback",
        "narrator",
        "screen reader says",
        "edit text",
    ];
    let wording = TEMPLATES
        .iter()
        .flat_map(|template| [template.id, template.pattern])
        .chain(ENDING_WORDING.iter().flat_map(|ending| {
            [ending.pattern, ending.unit.0, ending.unit.1]
                .into_iter()
                .chain(ending.zero_pattern)
        }))
        .chain([NO_ACCESSIBLE_NAME]);
    for text in wording {
        let lower = text.to_lowercase();
        for forbidden in FORBIDDEN {
            assert!(
                !lower.contains(forbidden),
                "{text:?} contains {forbidden:?}"
            );
        }
    }
    assert_eq!(
        serde_json::to_value(NameStatus::NoAccessibleName).expect("serialises"),
        NO_ACCESSIBLE_NAME
    );
}

#[test]
fn template_identifiers_are_unique_kebab_case() {
    let mut identifiers: Vec<&str> = TEMPLATES.iter().map(|template| template.id).collect();
    assert!(identifiers.iter().all(|id| {
        !id.is_empty()
            && id
                .chars()
                .all(|character| character.is_ascii_lowercase() || character == '-')
    }));
    identifiers.sort_unstable();
    identifiers.dedup();
    assert_eq!(identifiers.len(), TEMPLATES.len());
}

/// The accessible node of sign-in's element whose `id` attribute is `id`.
fn node_mut<'m>(model: &'m mut Model, id: &str) -> &'m mut AccessibleNode {
    model
        .nodes
        .iter_mut()
        .find_map(|node| match node {
            ExposedNode::Accessible(node)
                if node
                    .dom
                    .as_ref()
                    .is_some_and(|dom| dom.id.as_deref() == Some(id)) =>
            {
                Some(&mut **node)
            }
            ExposedNode::Accessible(_) | ExposedNode::TextRun(_) => None,
        })
        .unwrap_or_else(|| panic!("an exposed node with id {id:?}"))
}

#[test]
fn a_whitespace_only_name_is_no_accessible_name() {
    const BLANK: &str = "  \t\n ";
    let mut model = adapted("sign-in.json");
    node_mut(&mut model, "email")
        .name
        .as_mut()
        .expect("the textbox is named")
        .text = BLANK.to_owned();
    let transcript = with_layer_b(&model);
    let textbox = item(&transcript, "email");
    let record = accessible(textbox).expect("an accessible item");
    assert_eq!(record.name.status, NameStatus::NoAccessibleName);
    assert_eq!(
        record.name.text.as_deref(),
        Some(BLANK),
        "Layer A keeps the raw text"
    );
    let approx = textbox.approx.as_ref().expect("approx");
    assert_eq!(
        (approx.template, approx.phrase.as_str()),
        ("unnamed", "textbox")
    );
    let entry = transcript
        .indexes
        .form_controls
        .iter()
        .find(|entry| entry.item == textbox.number)
        .expect("the textbox is a form control");
    assert_eq!(*entry, unnamed_entry(textbox.number, "textbox"));
}

#[test]
fn an_empty_value_adds_no_part_and_a_value_does() {
    let phrase = |value: Option<&str>| {
        let mut model = adapted("sign-in.json");
        node_mut(&mut model, "email").value = value.map(str::to_owned);
        let transcript = with_layer_b(&model);
        let approx = item(&transcript, "email").approx.clone().expect("approx");
        approx.phrase
    };
    assert_eq!(phrase(None), "Email, textbox");
    assert_eq!(phrase(Some("")), phrase(None));
    assert_eq!(
        phrase(Some("you@example.com")),
        "Email, textbox, you@example.com"
    );
}

fn unresolved(backend_id: u64) -> FocusStop {
    FocusStop::Unresolved { backend_id }
}

/// A focus walk and the tab order it must give.
struct WalkCase {
    case: &'static str,
    steps: Vec<FocusStop>,
    truncated: bool,
    cycle_start: Option<u32>,
    kind: EndingKind,
    wording: &'static str,
    repeats: Vec<Option<usize>>,
}

/// Announces sign-in with the case's walk in place of its own and checks the ending, its
/// marking, and each step's `repeats`.
fn assert_walk(walk: WalkCase) {
    let mut model = adapted("sign-in.json");
    model.focus = Availability::Available(FocusWalk {
        steps: walk.steps,
        truncated: walk.truncated,
        cycle_start: walk.cycle_start,
    });
    let tab_order = with_layer_b(&model).tab_order.expect("a tab order");
    let repeats: Vec<Option<usize>> = tab_order.steps.iter().map(|step| step.repeats).collect();
    assert_eq!(
        (
            tab_order.ending.value.kind,
            tab_order.ending.value.wording.as_str(),
            repeats
        ),
        (walk.kind, walk.wording, walk.repeats),
        "{}",
        walk.case
    );
    assert_eq!(
        tab_order.ending.by,
        Marking::ComputedByTool,
        "{}",
        walk.case
    );
}

const WRAP_TO_STEP_1: &str =
    "Focus left the page and Tab returned to step 1, the normal end of a walk.";

#[test]
fn repeating_stops_are_matched_by_identity_on_synthetic_walks() {
    use FocusStop::LeftPage;
    let email = focus_of(&adapted("sign-in.json")).steps[0].clone();
    let cases = [
        WalkCase {
            case: "an unresolved stop repeats by backend id after leaving the page",
            steps: vec![unresolved(7), LeftPage, unresolved(7)],
            truncated: false,
            cycle_start: Some(2),
            kind: EndingKind::Wrap,
            wording: WRAP_TO_STEP_1,
            repeats: vec![None, None, Some(1)],
        },
        WalkCase {
            case: "an unresolved stop repeats by backend id without leaving the page",
            steps: vec![unresolved(7), unresolved(8), unresolved(7)],
            truncated: false,
            cycle_start: Some(2),
            kind: EndingKind::Trap,
            wording: "Tab returned to step 1 without focus leaving the page: a focus trap.",
            repeats: vec![None, None, Some(1)],
        },
        WalkCase {
            case: "an element stop repeats by DOM id",
            steps: vec![email.clone(), LeftPage, email],
            truncated: false,
            cycle_start: Some(2),
            kind: EndingKind::Wrap,
            wording: WRAP_TO_STEP_1,
            repeats: vec![None, None, Some(1)],
        },
        WalkCase {
            case: "the repeat names the earliest earlier matching step",
            steps: vec![unresolved(7), unresolved(7), LeftPage, unresolved(7)],
            truncated: false,
            cycle_start: Some(3),
            kind: EndingKind::Wrap,
            wording: WRAP_TO_STEP_1,
            repeats: vec![None, None, None, Some(1)],
        },
        WalkCase {
            case: "an unresolved stop with a different backend id is not a repeat",
            steps: vec![unresolved(8), unresolved(7), LeftPage, unresolved(7)],
            truncated: false,
            cycle_start: Some(3),
            kind: EndingKind::Wrap,
            wording: "Focus left the page and Tab returned to step 2, the normal end of a walk.",
            repeats: vec![None, None, None, Some(2)],
        },
    ];
    for case in cases {
        assert_walk(case);
    }
}

#[test]
fn walks_without_a_usable_cycle_end_by_the_other_rules() {
    use FocusStop::LeftPage;
    let cases = [
        WalkCase {
            case: "a cycle start past the last step is ignored",
            steps: vec![unresolved(7), LeftPage, unresolved(7)],
            truncated: false,
            cycle_start: Some(9),
            kind: EndingKind::Ended,
            wording: "The walk ended after 3 steps.",
            repeats: vec![None, None, None],
        },
        WalkCase {
            case: "a cycle start on the first step repeats nothing",
            steps: vec![unresolved(7), LeftPage, unresolved(7)],
            truncated: false,
            cycle_start: Some(0),
            kind: EndingKind::Ended,
            wording: "The walk ended after 3 steps.",
            repeats: vec![None, None, None],
        },
        WalkCase {
            case: "a cycle start on a left-page step repeats nothing",
            steps: vec![unresolved(7), LeftPage],
            truncated: false,
            cycle_start: Some(1),
            kind: EndingKind::LeftPage,
            wording: "Focus left the page after 1 element stop, with no return.",
            repeats: vec![None, None],
        },
        WalkCase {
            case: "focus leaves the page with no element stops",
            steps: vec![LeftPage, LeftPage],
            truncated: false,
            cycle_start: None,
            kind: EndingKind::LeftPage,
            wording: "Focus left the page without stopping on any element.",
            repeats: vec![None, None],
        },
        WalkCase {
            case: "truncation wins over a cycle",
            steps: vec![unresolved(7), LeftPage, unresolved(7)],
            truncated: true,
            cycle_start: Some(2),
            kind: EndingKind::Truncated,
            wording: "The walk stopped at its limit after 3 Tab presses. Later stops are not listed.",
            repeats: vec![None, None, Some(1)],
        },
        WalkCase {
            case: "an empty truncated walk",
            steps: Vec::new(),
            truncated: true,
            cycle_start: None,
            kind: EndingKind::Truncated,
            wording: "The walk stopped at its limit after 0 Tab presses. Later stops are not listed.",
            repeats: Vec::new(),
        },
    ];
    for case in cases {
        assert_walk(case);
    }
}

fn safari_ax_directory() -> PathBuf {
    fixtures().join("snapshots/safari-ax")
}

/// Every capture under `fixtures/snapshots/safari-ax/`, relative to it, found by listing the
/// directories, so a new capture without a render golden fails; `source-mapping.json` is a report.
fn safari_ax_names() -> Vec<PathBuf> {
    let root = safari_ax_directory();
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
                let relative = path.strip_prefix(&root).expect("listed under the root");
                names.push(relative.to_path_buf());
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

fn adapted_safari(name: &Path) -> Model {
    let snapshot = read_value(name, &value_at(&safari_ax_directory().join(name)));
    spokenpath_model::safari_ax::adapt(&snapshot, &source_lengths(name, &snapshot))
        .unwrap_or_else(|error| panic!("{} adapts: {error}", name.display()))
}

#[test]
fn every_safari_capture_matches_its_render_golden() {
    let names = safari_ax_names();
    assert!(names.len() >= 46, "{names:?}");
    for name in &names {
        let transcript = with_layer_b(&adapted_safari(name));
        let json = serde_json::to_string_pretty(&transcript).expect("a transcript serialises");
        insta::assert_snapshot!(format!("safari-ax__{}", snapshot_name(name)), json);
    }
}

#[test]
fn a_safari_transcript_says_which_parts_the_engine_does_not_supply() {
    let transcript = with_layer_b(&adapted_safari(Path::new("sign-in.json")));
    assert_eq!(
        transcript.not_available,
        [
            NotAvailablePart::NameSources,
            NotAvailablePart::Excluded,
            NotAvailablePart::TabOrder
        ]
    );
    let json = to_value(&transcript);
    assert_eq!(
        json["not_available"],
        json!(["name-sources", "excluded", "tab-order"])
    );
    assert!(json.get("excluded").is_none() && json.get("tab_order").is_none());
    for item in json["nodes"].as_array().expect("nodes") {
        assert!(item["layer_a"]["name"].get("sources").is_none(), "{item}");
    }
    assert_eq!(
        json["header"],
        json!({
            "engine": "safari-ax",
            "engine_version": "27.0.1",
            "tool_version": transcript.header.tool_version,
            "safari_build": "22625.1.29.11.28",
            "macos_version": "27.0.1",
            "macos_build": "26A434",
        })
    );
    // A Chromium transcript says nothing of the kind.
    let chromium = to_value(&with_layer_b(&adapted("sign-in.json")));
    assert!(chromium.get("not_available").is_none());
}

#[test]
fn a_platform_role_is_unspoken_and_never_indexed() {
    let transcript = with_layer_b(&adapted_safari(Path::new("sign-in.json")));
    let mut platform = 0;
    for item in &transcript.nodes {
        let Some(record) = accessible(item) else {
            continue;
        };
        let Some(role) = &record.role else {
            continue;
        };
        if role.kind != RoleKind::Platform {
            continue;
        }
        platform += 1;
        let approx = item.approx.as_ref().expect("Layer B");
        assert!(
            ["named-unspoken-role", "unnamed-unspoken-role"].contains(&approx.template),
            "item {}: {approx:?}",
            item.number
        );
        assert!(!approx.phrase.contains(&role.token), "{approx:?}");
    }
    assert!(platform > 3);
    let phrases: Vec<&str> = transcript
        .nodes
        .iter()
        .filter_map(|item| item.approx.as_ref().map(|approx| approx.phrase.as_str()))
        .collect();
    // The web area, the heading, the label and the email field.
    for phrase in [
        "Sign in",
        "Sign in, heading, level 1",
        "Email",
        "Email, textbox",
    ] {
        assert!(phrases.contains(&phrase), "{phrase}: {phrases:?}");
    }
    let indexes = &transcript.indexes;
    assert_eq!(indexes.headings.len(), 1);
    assert_eq!(indexes.links.len(), 1);
    let controls: Vec<&str> = indexes
        .form_controls
        .iter()
        .map(|entry| entry.role.as_str())
        .collect();
    assert_eq!(controls, ["textbox", "button"]);
    assert_eq!(indexes.landmarks, []);
}

/// The committed Chromium and Safari captures of the same page, with the hand-written pair.
fn aligned_pairs() -> Vec<(String, Model, Model)> {
    let mut pairs: Vec<(String, Model, Model)> = safari_ax_names()
        .into_iter()
        .map(|name| {
            (
                name.display().to_string(),
                adapted_path(&name),
                adapted_safari(&name),
            )
        })
        .collect();
    let snapshots = fixtures().join("snapshots");
    let read_at = |file: &str| read_value(Path::new(file), &value_at(&snapshots.join(file)));
    pairs.push((
        "basic".to_owned(),
        adapt(&read_at("chromium-basic.json"), &BTreeMap::new()).expect("adapts"),
        spokenpath_model::safari_ax::adapt(&read_at("safari-ax-basic.json"), &BTreeMap::new())
            .expect("adapts"),
    ));
    pairs
}

/// The source of an item located by a span: its path and start tag, which name the span, since
/// both snapshots of a pair hold the same spans.
fn span_of(item: &Item) -> Option<(String, u64, u64)> {
    let record = accessible(item)?;
    let range = record.source.start_tag?;
    if record.source.class != "span" {
        return None;
    }
    Some((record.source.path.clone()?, range.start, range.end))
}

#[test]
fn every_item_of_every_pair_is_in_exactly_one_row_and_aligned_rows_share_a_span() {
    let mut pairs_with_aligned_rows = 0;
    for (name, chromium_model, safari_model) in aligned_pairs() {
        let mut chromium = with_layer_b(&chromium_model);
        let safari = with_layer_b(&safari_model);
        crate::align(&mut chromium, &chromium_model, safari, &safari_model);
        let aligned = chromium.aligned.as_ref().expect("aligned");
        let rows = &aligned.rows;
        for (engine, items, count) in [
            (
                "chromium",
                rows.iter()
                    .flat_map(|row| row.chromium.iter())
                    .copied()
                    .collect::<Vec<_>>(),
                chromium.nodes.len(),
            ),
            (
                "safari",
                rows.iter()
                    .flat_map(|row| row.safari.iter())
                    .copied()
                    .collect::<Vec<_>>(),
                aligned.safari.nodes.len(),
            ),
        ] {
            let mut sorted = items.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, (1..=count).collect::<Vec<_>>(), "{name}: {engine}");
        }
        let item_of = |engine: &str, number: usize| {
            let nodes = if engine == "chromium" {
                &chromium.nodes
            } else {
                &aligned.safari.nodes
            };
            &nodes[number - 1]
        };
        let mut aligned_rows = 0;
        for row in rows {
            let spans: HashSet<Option<(String, u64, u64)>> = row
                .chromium
                .iter()
                .map(|&number| span_of(item_of("chromium", number)))
                .chain(
                    row.safari
                        .iter()
                        .map(|&number| span_of(item_of("safari", number))),
                )
                .collect();
            if !row.chromium.is_empty() && !row.safari.is_empty() {
                aligned_rows += 1;
                assert_eq!(spans.len(), 1, "{name}: {row:?}");
                assert!(spans.iter().all(Option::is_some), "{name}: {row:?}");
            }
            if row.chromium.len() + row.safari.len() > 1 {
                assert_eq!(
                    spans.len(),
                    1,
                    "{name}: a multi-item row shares one span: {row:?}"
                );
            }
        }
        if aligned_rows > 0 {
            pairs_with_aligned_rows += 1;
        }
    }
    // A page whose only tagged element Safari leaves out has none; most pages have some.
    assert!(pairs_with_aligned_rows > 40, "{pairs_with_aligned_rows}");
}

#[test]
fn rows_follow_chromium_order_and_safari_only_rows_follow_their_anchor() {
    for (name, chromium_model, safari_model) in aligned_pairs() {
        let mut chromium = with_layer_b(&chromium_model);
        crate::align(
            &mut chromium,
            &chromium_model,
            with_layer_b(&safari_model),
            &safari_model,
        );
        let rows = &chromium.aligned.as_ref().expect("aligned").rows;
        let firsts: Vec<usize> = rows
            .iter()
            .filter_map(|row| row.chromium.first().copied())
            .collect();
        assert!(is_increasing(&firsts), "{name}: {firsts:?}");
        let is_aligned =
            |row: &crate::AlignedRow| !row.chromium.is_empty() && !row.safari.is_empty();
        let first_aligned = rows.iter().position(is_aligned);
        // The row of each Safari item that is in an aligned row.
        let mut aligned_row_of: BTreeMap<usize, usize> = BTreeMap::new();
        for (position, row) in rows.iter().enumerate().filter(|(_, row)| is_aligned(row)) {
            for &item in &row.safari {
                aligned_row_of.insert(item, position);
            }
        }
        let mut previous_safari_only = 0;
        for (position, row) in rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.chromium.is_empty())
        {
            let first = row.safari[0];
            assert!(
                first > previous_safari_only,
                "{name}: Safari-only rows in Safari order"
            );
            previous_safari_only = first;
            let anchor = aligned_row_of
                .range(..first)
                .next_back()
                .map(|(_, &row)| row);
            match anchor {
                Some(anchor) => {
                    assert!(anchor < position, "{name}: {row:?}");
                    assert!(
                        !rows[anchor + 1..position].iter().any(is_aligned),
                        "{name}: {row:?} is placed after a later aligned row"
                    );
                }
                None => {
                    if let Some(first_aligned) = first_aligned {
                        assert!(position < first_aligned, "{name}: {row:?}");
                    }
                }
            }
            // Nothing Chromium-only from the anchor's run is left after it.
            if let Some(next) = rows.get(position + 1) {
                assert!(
                    !(next.safari.is_empty()
                        && first_aligned.is_some_and(|first| position > first)),
                    "{name}: a Chromium-only row follows {row:?}"
                );
            }
        }
    }
}
