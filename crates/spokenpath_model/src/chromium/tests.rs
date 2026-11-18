use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use spokenpath_snapshot::{Origin, Payload, Snapshot, read};

use super::adapt;
use crate::{
    AccessibleNode, AdaptError, DescriptionSource, ExcludedNode, ExclusionKind, ExposedNode,
    FocusStop, Model, NameSourceKind, NativeSource, PropertyValue, Role, RoleOrigin, SetPosition,
    SourceClass, Supplied, SuppliedBy, TextPartKind, Tristate,
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn goldens_directory() -> PathBuf {
    fixtures().join("snapshots/chromium")
}

/// Every golden under `fixtures/snapshots/chromium/`, relative to it, found by listing the
/// directories, so a new golden needs no test change.
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

fn adapted(name: &str) -> Model {
    let name = Path::new(name);
    let snapshot = read_value(name, &golden_value(name));
    adapt(&snapshot, &source_lengths(name, &snapshot))
        .unwrap_or_else(|error| panic!("{} adapts: {error}", name.display()))
}

/// Adapts a copy of a golden after `edit`, which must leave it readable, and returns the error.
fn adapt_error(name: &str, edit: impl FnOnce(&mut Value)) -> AdaptError {
    let name = Path::new(name);
    let mut value = golden_value(name);
    edit(&mut value);
    let snapshot = read_value(name, &value);
    adapt(&snapshot, &source_lengths(name, &snapshot)).expect_err("the edited snapshot is rejected")
}

fn assert_names_field(error: &AdaptError, path: &str) {
    let text = error.to_string();
    assert!(
        text.starts_with(&format!("invalid snapshot at `{path}`: ")),
        "expected an error at `{path}`, got: {text}"
    );
}

fn accessible_nodes(model: &Model) -> impl Iterator<Item = &AccessibleNode> {
    model.nodes.iter().filter_map(ExposedNode::as_accessible)
}

/// The exposed node of the element whose `id` attribute is `id`.
fn exposed<'m>(model: &'m Model, id: &str) -> &'m AccessibleNode {
    accessible_nodes(model)
        .find(|node| {
            node.dom
                .as_ref()
                .is_some_and(|dom| dom.id.as_deref() == Some(id))
        })
        .unwrap_or_else(|| panic!("an exposed node with id {id:?}"))
}

/// The excluded node of the element whose `id` attribute is `id`.
fn excluded<'m>(model: &'m Model, id: &str) -> &'m ExcludedNode {
    model
        .excluded
        .iter()
        .find(|node| node.dom.id.as_deref() == Some(id))
        .unwrap_or_else(|| panic!("an excluded node with id {id:?}"))
}

fn excluded_by_tag<'m>(model: &'m Model, tag: &str) -> Vec<&'m ExcludedNode> {
    model
        .excluded
        .iter()
        .filter(|node| node.dom.tag.as_deref() == Some(tag))
        .collect()
}

fn ax_position(value: &Value, predicate: impl Fn(&Value) -> bool) -> usize {
    value["payload"]["ax"]
        .as_array()
        .expect("an ax array")
        .iter()
        .position(predicate)
        .expect("a matching ax node")
}

#[test]
fn every_golden_reads_and_adapts_with_sources_classified() {
    let names = golden_names();
    for directory in ["focus", "names", "set-position", "excluded"] {
        assert!(
            names.iter().any(|name| name.starts_with(directory)),
            "no golden under {directory}/"
        );
    }
    for name in &names {
        let value = golden_value(name);
        let snapshot = read_value(name, &value);
        let model = adapt(&snapshot, &source_lengths(name, &snapshot))
            .unwrap_or_else(|error| panic!("{} adapts: {error}", name.display()));
        let Payload::Chromium(payload) = &snapshot.payload else {
            panic!("{} is a Chromium capture", name.display());
        };
        let label = name.display();

        assert!(
            matches!(model.node(model.root), Some(ExposedNode::Accessible(_))),
            "{label}: the root is an accessible node"
        );
        for (index, node) in model.indexed_nodes() {
            for &child in node.children() {
                assert_eq!(
                    model.node(child).and_then(ExposedNode::parent),
                    Some(index),
                    "{label}: children name their parent"
                );
            }
            let runs = node
                .children()
                .windows(2)
                .filter(|pair| {
                    pair.iter()
                        .all(|&child| matches!(model.node(child), Some(ExposedNode::TextRun(_))))
                })
                .count();
            assert_eq!(runs, 0, "{label}: adjacent text runs are merged");
        }
        for node in accessible_nodes(&model) {
            let role = node.role.as_ref().map(Role::as_str);
            assert!(
                !matches!(role, Some("InlineTextBox" | "StaticText" | "LineBreak")),
                "{label}: {role:?} is never an accessible node"
            );
            assert_ne!(node.source.as_str(), "");
            let Some(dom) = &node.dom else {
                assert_eq!(node.source, SourceClass::NotInDom, "{label}");
                continue;
            };
            let raw = payload
                .dom
                .iter()
                .find(|raw| raw.id == dom.dom_id)
                .expect("the DOM node exists");
            if raw.origin == Some(Origin::Source) {
                let span_id = raw.span.expect("a source element has a span");
                let span = snapshot
                    .spans
                    .iter()
                    .find(|span| span.id == span_id)
                    .expect("the span exists");
                let source = snapshot
                    .sources
                    .iter()
                    .find(|source| source.id == span.source)
                    .expect("the source exists");
                let SourceClass::Span(reference) = &node.source else {
                    panic!("{label}: node {} has no span reference", dom.dom_id);
                };
                assert_eq!(
                    (reference.path.as_str(), reference.line, reference.column),
                    (source.path.as_str(), span.line, span.column),
                    "{label}: node {}",
                    dom.dom_id
                );
            }
        }
        for node in &model.excluded {
            assert_ne!(node.source.as_str(), "");
            assert_eq!(
                (&node.role, &node.name),
                (&None, &None),
                "{label}: an excluded node's role and name are never filled in"
            );
        }
    }
}

#[test]
fn long_page_adapts_well_under_a_second() {
    let name = Path::new("focus/long.json");
    let snapshot = read_value(name, &golden_value(name));
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

#[test]
fn implied_and_script_created_elements_are_classified_by_origin() {
    let model = adapted("origins.json");
    let classes: Vec<(Option<&str>, &SourceClass)> = accessible_nodes(&model)
        .filter_map(|node| {
            node.dom
                .as_ref()
                .map(|dom| (dom.tag.as_deref(), &node.source))
        })
        .chain(
            model
                .excluded
                .iter()
                .map(|node| (node.dom.tag.as_deref(), &node.source)),
        )
        .collect();
    assert!(
        classes.contains(&(Some("tbody"), &SourceClass::Implied)),
        "{classes:?}"
    );
    let button = exposed(&model, "added");
    assert_eq!(button.source, SourceClass::Script);
    assert_eq!(
        button.name.as_ref().map(|name| name.text.as_str()),
        Some("Added by script")
    );
    assert!(matches!(
        exposed(&model, "table").source,
        SourceClass::Span(_)
    ));
}

#[test]
fn user_agent_shadow_content_is_classified_unknown() {
    let model = adapted("focus/date-input.json");
    let spinbuttons: Vec<&AccessibleNode> = accessible_nodes(&model)
        .filter(|node| node.role == Some(Role::Aria("spinbutton".to_owned())))
        .collect();
    assert_eq!(spinbuttons.len(), 3);
    for spinbutton in spinbuttons {
        assert_eq!(spinbutton.source, SourceClass::Unknown);
        assert_eq!(spinbutton.value.as_deref(), Some("0"));
    }
}

#[test]
fn pseudo_element_list_markers_are_classified_not_in_dom() {
    let model = adapted("set-position/list.json");
    let markers: Vec<&AccessibleNode> = accessible_nodes(&model)
        .filter(|node| node.role == Some(Role::Internal("ListMarker".to_owned())))
        .collect();
    assert_eq!(markers.len(), 3);
    for marker in markers {
        assert_eq!(
            (&marker.dom, &marker.source),
            (&None, &SourceClass::NotInDom)
        );
        assert_eq!(marker.role_origin, None);
    }
}

#[test]
fn text_nodes_become_runs_that_list_their_dom_text_nodes() {
    let model = adapted("excluded/aria-hidden.json");
    let button = exposed(&model, "before");
    let [child] = button.children.as_slice() else {
        panic!("one child: {:?}", button.children);
    };
    let run = model
        .node(*child)
        .and_then(ExposedNode::as_text_run)
        .expect("a text run");
    assert_eq!(run.text, "Before");
    assert_eq!(run.parts.len(), 1);
    assert_eq!(run.parts[0].kind, TextPartKind::Text);
    assert_eq!(run.parts[0].source, SourceClass::Text);
    assert_eq!(
        run.parts[0].dom.as_ref().and_then(|dom| dom.tag.as_ref()),
        None
    );
}

#[test]
fn adjacent_text_and_line_breaks_merge_into_one_run() {
    // A paragraph whose children are text, a line break and text: the run joins them in order.
    let name = Path::new("excluded/aria-hidden.json");
    let mut value = golden_value(name);
    let button = ax_position(&value, |node| node["name"]["value"] == json!("Before"));
    let text = value["payload"]["ax"][button]["childIds"][0].clone();
    let text_position = ax_position(&value, |node| node["nodeId"] == text);
    let mut line_break = value["payload"]["ax"][text_position].clone();
    line_break["nodeId"] = json!("line-break");
    line_break["role"] = json!({"type": "internalRole", "value": "LineBreak"});
    line_break["childIds"] = json!([]);
    let mut second = value["payload"]["ax"][text_position].clone();
    second["nodeId"] = json!("second-text");
    second["name"]["value"] = json!("After");
    second["childIds"] = json!([]);
    let ax = value["payload"]["ax"].as_array_mut().expect("an ax array");
    ax.push(line_break);
    ax.push(second);
    let children = value["payload"]["ax"][button]["childIds"]
        .as_array_mut()
        .expect("child ids");
    children.push(json!("line-break"));
    children.push(json!("second-text"));

    let snapshot = read_value(name, &value);
    let model = adapt(&snapshot, &source_lengths(name, &snapshot)).expect("adapts");
    let button = exposed(&model, "before");
    let [child] = button.children.as_slice() else {
        panic!("one merged run: {:?}", button.children);
    };
    let run = model
        .node(*child)
        .and_then(ExposedNode::as_text_run)
        .expect("a text run");
    assert_eq!(run.text, "Before\nAfter");
    let kinds: Vec<TextPartKind> = run.parts.iter().map(|part| part.kind).collect();
    assert_eq!(
        kinds,
        [
            TextPartKind::Text,
            TextPartKind::LineBreak,
            TextPartKind::Text
        ]
    );
}

#[test]
fn children_of_ignored_nodes_take_their_place_in_the_parent() {
    // `html` and `body` are ignored, so the paragraphs are the root's own children.
    let model = adapted("sign-in.json");
    let root = model.node(model.root).expect("the root");
    let root = root.as_accessible().expect("an accessible root");
    assert_eq!(root.role, Some(Role::Internal("RootWebArea".to_owned())));
    assert_eq!(root.role_origin, None);
    assert_eq!(root.source, SourceClass::Document);
    let tags: Vec<Option<&str>> = root
        .children
        .iter()
        .filter_map(|&child| model.node(child).and_then(ExposedNode::as_accessible))
        .map(|child| child.dom.as_ref().and_then(|dom| dom.tag.as_deref()))
        .collect();
    assert_eq!(tags[0], Some("h1"));
    assert!(tags[1..].iter().all(|tag| *tag == Some("p")), "{tags:?}");
    let excluded_tags: Vec<&str> = model
        .excluded
        .iter()
        .filter(|node| {
            node.reasons
                .iter()
                .any(|reason| reason.kind == ExclusionKind::Uninteresting)
        })
        .filter_map(|node| node.dom.tag.as_deref())
        .collect();
    assert!(excluded_tags.starts_with(&["html"]), "{excluded_tags:?}");
    assert!(excluded_tags.contains(&"body"), "{excluded_tags:?}");
}

#[test]
fn label_for_names_the_control_with_the_label_as_related_node() {
    let model = adapted("names/label-for.json");
    let name = exposed(&model, "target").name.as_ref().expect("a name");
    assert_eq!(name.text, "Label for text");
    let winner = name.winning_source().expect("a winning source");
    assert_eq!(winner.kind, NameSourceKind::RelatedElement);
    assert_eq!(winner.native_source, Some(NativeSource::LabelFor));
    assert_eq!(winner.related_nodes.len(), 1);
    let label = winner.related_nodes[0]
        .dom
        .as_ref()
        .expect("the label resolves");
    assert_eq!(
        (label.tag.as_deref(), label.id.as_deref()),
        (Some("label"), Some("label"))
    );
    assert_eq!(name.from_hidden_content, Some(Supplied::tool(false)));
}

#[test]
fn wrapping_label_names_the_control() {
    let model = adapted("names/label-wrapped.json");
    let name = exposed(&model, "target").name.as_ref().expect("a name");
    assert_eq!(name.text, "Wrapped label text");
    let winner = name.winning_source().expect("a winning source");
    assert_eq!(winner.kind, NameSourceKind::RelatedElement);
    assert_eq!(winner.native_source, Some(NativeSource::LabelWrapped));
}

#[test]
fn labelledby_wins_over_aria_label_which_stays_superseded_in_the_chain() {
    let model = adapted("names/labelledby-over-label.json");
    let name = exposed(&model, "target").name.as_ref().expect("a name");
    assert_eq!(name.text, "Labelledby text");
    assert_eq!(name.winner, Some(0));
    let winner = name.winning_source().expect("a winning source");
    assert_eq!(winner.kind, NameSourceKind::RelatedElement);
    assert_eq!(winner.attribute.as_deref(), Some("aria-labelledby"));
    assert_eq!(winner.attribute_value.as_deref(), Some("label"));
    let span = winner.related_nodes[0]
        .dom
        .as_ref()
        .expect("the span resolves");
    assert_eq!(
        (span.tag.as_deref(), span.id.as_deref()),
        (Some("span"), Some("label"))
    );
    let aria_label = &name.sources[1];
    assert_eq!(aria_label.kind, NameSourceKind::Attribute);
    assert_eq!(aria_label.attribute.as_deref(), Some("aria-label"));
    assert_eq!(
        aria_label.attribute_value.as_deref(),
        Some("Aria label text")
    );
    assert_eq!(aria_label.value.as_deref(), Some("Aria label text"));
    assert!(aria_label.superseded);
}

#[test]
fn aria_label_names_the_button() {
    let model = adapted("names/aria-label.json");
    let name = exposed(&model, "target").name.as_ref().expect("a name");
    assert_eq!(name.text, "Aria label text");
    let winner = name.winning_source().expect("a winning source");
    assert_eq!(winner.kind, NameSourceKind::Attribute);
    assert_eq!(winner.attribute.as_deref(), Some("aria-label"));
    assert_eq!(name.from_hidden_content, None);
}

#[test]
fn title_names_the_input() {
    let model = adapted("names/title.json");
    let target = exposed(&model, "target");
    let name = target.name.as_ref().expect("a name");
    assert_eq!(name.text, "Title text");
    let winner = name.winning_source().expect("a winning source");
    assert_eq!(winner.kind, NameSourceKind::Attribute);
    assert_eq!(winner.attribute.as_deref(), Some("title"));
    let titles = name
        .sources
        .iter()
        .filter(|source| source.attribute.as_deref() == Some("title"))
        .count();
    assert_eq!(titles, 2, "the engine lists title twice and both are kept");
    assert_eq!(target.states.invalid.as_deref(), Some("false"));
}

#[test]
fn placeholder_names_the_input() {
    let model = adapted("names/placeholder.json");
    let name = exposed(&model, "target").name.as_ref().expect("a name");
    assert_eq!(name.text, "Placeholder text");
    let winner = name.winning_source().expect("a winning source");
    assert_eq!(winner.kind, NameSourceKind::Placeholder);
    assert_eq!(winner.attribute.as_deref(), Some("placeholder"));
}

#[test]
fn alt_names_the_image() {
    let model = adapted("names/img-alt.json");
    let target = exposed(&model, "target");
    assert_eq!(target.role, Some(Role::Aria("image".to_owned())));
    let name = target.name.as_ref().expect("a name");
    assert_eq!(name.text, "Alt text");
    let winner = name.winning_source().expect("a winning source");
    assert_eq!(winner.kind, NameSourceKind::Attribute);
    assert_eq!(winner.attribute.as_deref(), Some("alt"));
    assert_eq!(
        target
            .properties
            .iter()
            .find(|property| property.name == "url")
            .map(|property| &property.value),
        Some(&PropertyValue::Text(
            "http://spokenpath.test/pixel.svg".to_owned()
        ))
    );
}

#[test]
fn labelledby_to_a_hidden_element_is_a_name_from_hidden_content() {
    let model = adapted("names/labelledby-hidden.json");
    let name = exposed(&model, "target").name.as_ref().expect("a name");
    assert_eq!(name.text, "Hidden label text");
    let winner = name.winning_source().expect("a winning source");
    assert_eq!(winner.attribute.as_deref(), Some("aria-labelledby"));
    let related = winner.related_nodes[0]
        .dom
        .as_ref()
        .expect("the span resolves");
    assert_eq!(related.id.as_deref(), Some("label"));
    let label = excluded(&model, "label");
    assert_eq!(label.reasons[0].kind, ExclusionKind::NotRendered);
    assert_eq!(
        name.from_hidden_content,
        Some(Supplied {
            value: true,
            by: SuppliedBy::Tool
        })
    );
}

#[test]
fn icon_button_has_an_empty_name_with_no_winner_and_its_svg_excluded() {
    let model = adapted("names/icon-button.json");
    let name = exposed(&model, "target").name.as_ref().expect("a name");
    assert_eq!(name.text, "");
    assert_eq!(name.winner, None);
    assert_eq!(name.from_hidden_content, None);
    let tried: Vec<&NameSourceKind> = name.sources.iter().map(|source| &source.kind).collect();
    assert_eq!(
        tried,
        [
            &NameSourceKind::RelatedElement,
            &NameSourceKind::Attribute,
            &NameSourceKind::RelatedElement,
            &NameSourceKind::Contents,
            &NameSourceKind::Attribute,
        ]
    );
    assert!(name.sources.iter().all(|source| source.value.is_none()));
    let icon = excluded(&model, "icon");
    assert_eq!(icon.reasons.len(), 1);
    assert_eq!(icon.reasons[0].kind, ExclusionKind::AriaHidden);
    assert_eq!(icon.reasons[0].engine_reason, "ariaHiddenElement");
    assert_eq!(icon.reasons[0].hiding_ancestor, None);
    let [path] = excluded_by_tag(&model, "path")[..] else {
        panic!("one path");
    };
    assert_eq!(path.reasons[0].engine_reason, "ariaHiddenSubtree");
    let ancestor = path.reasons[0]
        .hiding_ancestor
        .as_ref()
        .expect("a hiding ancestor");
    assert_eq!(ancestor.by, SuppliedBy::Engine);
    assert_eq!(ancestor.value.id.as_deref(), Some("icon"));
}

#[test]
fn sign_in_textbox_is_named_by_its_label_and_the_svg_button_is_unnamed() {
    let model = adapted("sign-in.json");
    let email = exposed(&model, "email");
    assert_eq!(email.role, Some(Role::Aria("textbox".to_owned())));
    assert_eq!(email.role_origin, Some(Supplied::tool(RoleOrigin::Native)));
    let name = email.name.as_ref().expect("a name");
    assert_eq!(name.text, "Email");
    let winner = name.winning_source().expect("a winning source");
    assert_eq!(winner.native_source, Some(NativeSource::LabelFor));
    assert_eq!(
        winner.related_nodes[0]
            .dom
            .as_ref()
            .and_then(|dom| dom.tag.as_deref()),
        Some("label")
    );
    assert_eq!(email.states.focusable, Some(true));
    assert_eq!(email.states.required, Some(false));
    assert_eq!(email.states.readonly, Some(false));

    let submit = exposed(&model, "submit");
    let name = submit.name.as_ref().expect("a name");
    assert_eq!((name.text.as_str(), name.winner), ("", None));
}

#[test]
fn describedby_description_names_the_resolved_element() {
    let model = adapted("names/description-describedby.json");
    let description = exposed(&model, "target")
        .description
        .as_ref()
        .expect("a description");
    assert_eq!(description.text, "Saves the draft");
    let provenance = description.provenance.as_ref().expect("a provenance");
    assert_eq!(provenance.by, SuppliedBy::Tool);
    let DescriptionSource::AriaDescribedby(elements) = &provenance.value else {
        panic!("aria-describedby: {provenance:?}");
    };
    let ids: Vec<Option<&str>> = elements
        .iter()
        .map(|element| element.id.as_deref())
        .collect();
    assert_eq!(ids, [Some("hint")]);
    let describedby = exposed(&model, "target")
        .properties
        .iter()
        .find(|property| property.name == "describedby")
        .expect("the describedby property is kept");
    let PropertyValue::Nodes(nodes) = &describedby.value else {
        panic!("a node list: {describedby:?}");
    };
    assert_eq!(nodes[0].idref.as_deref(), Some("hint"));
}

#[test]
fn aria_description_is_attributed_to_the_attribute() {
    let model = adapted("names/description-aria-description.json");
    let description = exposed(&model, "target")
        .description
        .as_ref()
        .expect("a description");
    assert_eq!(
        description.provenance,
        Some(Supplied::tool(DescriptionSource::AriaDescription))
    );
}

#[test]
fn title_description_is_attributed_to_title_when_the_name_does_not_use_it() {
    let model = adapted("names/description-title.json");
    let target = exposed(&model, "target");
    assert_eq!(
        target
            .name
            .as_ref()
            .and_then(|name| name.winning_source())
            .map(|source| &source.kind),
        Some(&NameSourceKind::Contents)
    );
    let description = target.description.as_ref().expect("a description");
    assert_eq!(
        description.provenance,
        Some(Supplied::tool(DescriptionSource::Title))
    );
}

#[test]
fn description_with_no_matching_attribute_has_no_provenance() {
    let name = Path::new("names/description-aria-description.json");
    let mut value = golden_value(name);
    for node in value["payload"]["dom"].as_array_mut().expect("a dom array") {
        if let Some(attributes) = node["attributes"].as_array_mut() {
            attributes.retain(|attribute| attribute["name"] != "aria-description");
        }
    }
    let snapshot = read_value(name, &value);
    let model = adapt(&snapshot, &source_lengths(name, &snapshot)).expect("adapts");
    let description = exposed(&model, "target")
        .description
        .as_ref()
        .expect("a description");
    assert_eq!(description.text, "Saves the draft");
    assert_eq!(description.provenance, None);
}

fn set_position(model: &Model, id: &str) -> Option<SetPosition> {
    let node = exposed(model, id);
    node.set_position.as_ref().map(|position| {
        assert_eq!(position.by, SuppliedBy::Tool, "{id}: computed by tool");
        position.value
    })
}

fn of(position: u32, size: u32) -> SetPosition {
    SetPosition {
        position,
        size: Some(size),
    }
}

#[test]
fn list_items_count_without_the_hidden_item() {
    let model = adapted("set-position/list.json");
    assert_eq!(set_position(&model, "first"), Some(of(1, 3)));
    assert_eq!(set_position(&model, "second"), Some(of(2, 3)));
    assert_eq!(set_position(&model, "third"), Some(of(3, 3)));
    assert_eq!(
        excluded(&model, "hidden").reasons[0].kind,
        ExclusionKind::NotRendered
    );
    assert_eq!(set_position(&model, "list"), None);
    assert_eq!(exposed(&model, "first").states.level, Some(1));
}

#[test]
fn native_radios_count_by_name_and_aria_radios_by_radiogroup() {
    let model = adapted("set-position/radio-group.json");
    assert_eq!(set_position(&model, "small"), Some(of(1, 3)));
    assert_eq!(set_position(&model, "medium"), Some(of(2, 3)));
    assert_eq!(set_position(&model, "large"), Some(of(3, 3)));
    assert_eq!(set_position(&model, "red"), Some(of(1, 2)));
    assert_eq!(set_position(&model, "blue"), Some(of(2, 2)));
    let red = exposed(&model, "red");
    assert_eq!(red.states.checked, Some(Tristate::True));
    assert_eq!(red.role_origin, Some(Supplied::tool(RoleOrigin::Author)));
    let small = exposed(&model, "small");
    assert_eq!(small.states.checked, Some(Tristate::False));
    assert_eq!(small.role_origin, Some(Supplied::tool(RoleOrigin::Native)));
}

#[test]
fn listbox_options_count_and_the_selected_option_is_selected() {
    let model = adapted("set-position/listbox.json");
    assert_eq!(set_position(&model, "apple"), Some(of(1, 3)));
    assert_eq!(set_position(&model, "pear"), Some(of(2, 3)));
    assert_eq!(set_position(&model, "plum"), Some(of(3, 3)));
    assert_eq!(exposed(&model, "pear").states.selected, Some(true));
    assert_eq!(exposed(&model, "apple").states.selected, Some(false));
    let orientation = exposed(&model, "fruit")
        .properties
        .iter()
        .find(|property| property.name == "orientation")
        .map(|property| &property.value);
    assert_eq!(
        orientation,
        Some(&PropertyValue::Token("vertical".to_owned()))
    );
}

fn with_attribute(value: &mut Value, id: &str, name: &str, attribute_value: &str) {
    let node = value["payload"]["dom"]
        .as_array_mut()
        .expect("a dom array")
        .iter_mut()
        .find(|node| {
            node["attributes"]
                .as_array()
                .is_some_and(|attributes| attributes.contains(&json!({"name": "id", "value": id})))
        })
        .expect("an element with the id");
    node["attributes"]
        .as_array_mut()
        .expect("attributes")
        .push(json!({"name": name, "value": attribute_value}));
}

#[test]
fn author_posinset_and_setsize_replace_the_counted_position() {
    let name = Path::new("set-position/listbox.json");
    let mut value = golden_value(name);
    with_attribute(&mut value, "apple", "aria-posinset", "7");
    with_attribute(&mut value, "apple", "aria-setsize", "40");
    with_attribute(&mut value, "pear", "aria-setsize", "-1");
    with_attribute(&mut value, "plum", "aria-posinset", "zero");
    let snapshot = read_value(name, &value);
    let model = adapt(&snapshot, &source_lengths(name, &snapshot)).expect("adapts");
    assert_eq!(set_position(&model, "apple"), Some(of(7, 40)));
    assert_eq!(
        set_position(&model, "pear"),
        Some(SetPosition {
            position: 2,
            size: None
        })
    );
    assert_eq!(set_position(&model, "plum"), Some(of(3, 3)));
}

#[test]
fn native_radio_without_a_name_is_a_set_of_one() {
    let name = Path::new("set-position/radio-group.json");
    let mut value = golden_value(name);
    for node in value["payload"]["dom"].as_array_mut().expect("a dom array") {
        let is_small = node["attributes"].as_array().is_some_and(|attributes| {
            attributes.contains(&json!({"name": "id", "value": "small"}))
        });
        if is_small {
            node["attributes"]
                .as_array_mut()
                .expect("attributes")
                .retain(|attribute| attribute["name"] != "name");
        }
    }
    let snapshot = read_value(name, &value);
    let model = adapt(&snapshot, &source_lengths(name, &snapshot)).expect("adapts");
    assert_eq!(set_position(&model, "small"), Some(of(1, 1)));
    assert_eq!(set_position(&model, "medium"), Some(of(1, 2)));
    assert_eq!(set_position(&model, "large"), Some(of(2, 2)));
}

fn assert_hidden_by_container(name: &str, kind: ExclusionKind, by: SuppliedBy) {
    let model = adapted(name);
    let button = excluded(&model, "hidden");
    let reason = button
        .reasons
        .iter()
        .find(|reason| reason.kind == kind)
        .unwrap_or_else(|| panic!("{name}: a {} reason: {:?}", kind.as_str(), button.reasons));
    let ancestor = reason.hiding_ancestor.as_ref().expect("a hiding ancestor");
    assert_eq!(ancestor.by, by, "{name}");
    assert_eq!(
        (ancestor.value.tag.as_deref(), ancestor.value.id.as_deref()),
        (Some("div"), Some("container")),
        "{name}"
    );
    let container = excluded(&model, "container");
    assert!(
        container
            .reasons
            .iter()
            .all(|reason| reason.hiding_ancestor.is_none()),
        "{name}: the container hides itself: {:?}",
        container.reasons
    );
    assert_eq!(container.reasons[0].kind, kind, "{name}");
    assert!(matches!(button.source, SourceClass::Span(_)));
    assert_eq!((&button.role, &button.name), (&None, &None));
}

#[test]
fn aria_hidden_subtree_names_the_container_as_supplied_by_the_engine() {
    assert_hidden_by_container(
        "excluded/aria-hidden.json",
        ExclusionKind::AriaHidden,
        SuppliedBy::Engine,
    );
}

#[test]
fn inert_subtree_finds_the_inert_container_computed_by_tool() {
    assert_hidden_by_container(
        "excluded/inert.json",
        ExclusionKind::Inert,
        SuppliedBy::Tool,
    );
}

#[test]
fn display_none_subtree_finds_the_unrendered_container_computed_by_tool() {
    assert_hidden_by_container(
        "excluded/display-none.json",
        ExclusionKind::NotRendered,
        SuppliedBy::Tool,
    );
}

#[test]
fn visibility_hidden_subtree_finds_the_hidden_container_computed_by_tool() {
    assert_hidden_by_container(
        "excluded/visibility-hidden.json",
        ExclusionKind::NotVisible,
        SuppliedBy::Tool,
    );
}

#[test]
fn presentational_heading_is_excluded_with_no_hiding_ancestor() {
    let model = adapted("excluded/presentational-role.json");
    let heading = excluded(&model, "target");
    assert_eq!(heading.dom.tag.as_deref(), Some("h2"));
    assert_eq!(heading.reasons.len(), 1);
    assert_eq!(heading.reasons[0].kind, ExclusionKind::PresentationalRole);
    assert_eq!(heading.reasons[0].hiding_ancestor, None);
    assert_eq!((&heading.role, &heading.name), (&None, &None));
}

#[test]
fn inert_focus_page_names_the_inert_div_for_its_buttons() {
    let model = adapted("focus/inert.json");
    for id in ["inert-button", "inert-link"] {
        let node = excluded(&model, id);
        assert_eq!(node.reasons[0].kind, ExclusionKind::Inert);
        let ancestor = node.reasons[0]
            .hiding_ancestor
            .as_ref()
            .expect("a hiding ancestor");
        assert_eq!(ancestor.by, SuppliedBy::Tool);
        assert_eq!(ancestor.value.tag.as_deref(), Some("div"));
    }
}

#[test]
fn head_children_name_head_as_their_hiding_ancestor() {
    let model = adapted("sign-in.json");
    let [head] = excluded_by_tag(&model, "head")[..] else {
        panic!("one head");
    };
    assert_eq!(head.reasons[0].kind, ExclusionKind::NotRendered);
    assert_eq!(head.reasons[0].hiding_ancestor, None);
    for tag in ["meta", "title"] {
        let [node] = excluded_by_tag(&model, tag)[..] else {
            panic!("one {tag}");
        };
        let ancestor = node.reasons[0]
            .hiding_ancestor
            .as_ref()
            .expect("a hiding ancestor");
        assert_eq!(ancestor.by, SuppliedBy::Tool);
        assert_eq!(ancestor.value.dom_id, head.dom.dom_id, "{tag}");
    }
}

#[test]
fn without_a_dom_snapshot_layout_hiding_ancestors_are_absent() {
    let name = Path::new("excluded/display-none.json");
    let mut value = golden_value(name);
    value["payload"]
        .as_object_mut()
        .expect("a payload")
        .remove("dom_snapshot");
    let snapshot = read_value(name, &value);
    let model = adapt(&snapshot, &source_lengths(name, &snapshot)).expect("adapts");
    let button = excluded(&model, "hidden");
    assert_eq!(button.reasons[0].kind, ExclusionKind::NotRendered);
    assert_eq!(button.reasons[0].hiding_ancestor, None);
}

#[test]
fn element_with_no_accessibility_node_is_excluded_with_no_reasons() {
    let name = Path::new("excluded/presentational-role.json");
    let mut value = golden_value(name);
    value["payload"]["ax_forced"] = json!([]);
    let snapshot = read_value(name, &value);
    let model = adapt(&snapshot, &source_lengths(name, &snapshot)).expect("adapts");
    assert_eq!(excluded(&model, "target").reasons, []);
}

#[test]
fn focus_walk_keeps_unresolved_steps_and_links_exposed_elements() {
    let model = adapted("focus/added-on-tab.json");
    let steps = &model.focus.steps;
    assert_eq!(steps.len(), 4);
    let FocusStop::Element { dom, node } = &steps[0] else {
        panic!("an element: {:?}", steps[0]);
    };
    let node = node.expect("the focused element is exposed");
    assert_eq!(
        model
            .node(node)
            .and_then(ExposedNode::as_accessible)
            .and_then(|node| node.dom.as_ref()),
        Some(dom)
    );
    assert!(matches!(steps[1], FocusStop::Unresolved { .. }));
    assert_eq!(steps[2], FocusStop::LeftPage);
    assert_eq!(model.focus.cycle_start, Some(3));
    assert!(!model.focus.truncated);
    assert_eq!(model.diagnostics.len(), 1);
    assert_eq!(model.diagnostics[0].code, "focus-target-not-in-dom");
    assert_eq!(model.diagnostics[0].dom, None);
}

#[test]
fn focus_on_an_excluded_element_has_no_exposed_node() {
    let model = adapted("excluded/aria-hidden.json");
    let FocusStop::Element { dom, node } = &model.focus.steps[1] else {
        panic!("an element: {:?}", model.focus.steps[1]);
    };
    assert_eq!(dom.id.as_deref(), Some("hidden"));
    assert_eq!(*node, None);
}

#[test]
fn model_carries_the_capture_versions() {
    let model = adapted("sign-in.json");
    assert_eq!(model.engine, spokenpath_snapshot::Engine::Chromium);
    assert_eq!(model.engine_version, "159.0.8154.6");
    assert_eq!(model.tool_version, "playwright 1.65.1");
}

#[test]
fn safari_payload_is_rejected_naming_its_engine() {
    let name = Path::new("sign-in.json");
    let mut value = golden_value(name);
    value["capture"]["engine"] = json!("safari-ax");
    value["payload"] = json!({"engine": "safari-ax"});
    let snapshot = read_value(name, &value);
    let error = adapt(&snapshot, &BTreeMap::new()).expect_err("a Safari payload is rejected");
    insta::assert_snapshot!(error, @"the chromium adapter cannot read a snapshot whose payload is for `safari-ax`");
}

#[test]
fn dangling_span_source_names_the_span() {
    let error = adapt_error("sign-in.json", |value| {
        value["spans"][2]["source"] = json!(9);
    });
    insta::assert_snapshot!(error, @"invalid snapshot at `spans[2].source`: names source 9, which is not in `sources`");
}

#[test]
fn dangling_dom_span_names_the_node() {
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["dom"][8]["span"] = json!(999);
    });
    assert_names_field(&error, "payload.dom[8].span");
}

#[test]
fn dangling_dom_parent_names_the_node() {
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["dom"][8]["parent"] = json!(999);
    });
    assert_names_field(&error, "payload.dom[8].parent");
}

#[test]
fn dangling_dom_child_names_the_child_entry() {
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["dom"][7]["children"][1] = json!(999);
    });
    insta::assert_snapshot!(error, @"invalid snapshot at `payload.dom[7].children[1]`: names node 999, which is not in `payload.dom`");
}

#[test]
fn child_with_another_parent_names_the_child_entry() {
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["dom"][9]["parent"] = json!(7);
    });
    assert_names_field(&error, "payload.dom[8].children[0]");
}

#[test]
fn dom_cycle_is_reported_rather_than_followed() {
    // The heading and its text node point at each other, and the body no longer lists the heading.
    let error = adapt_error("sign-in.json", |value| {
        let dom = &mut value["payload"]["dom"];
        let body_children = dom[7]["children"].as_array_mut().expect("children");
        body_children.retain(|child| *child != json!(8));
        dom[8]["parent"] = json!(9);
        dom[9]["children"]
            .as_array_mut()
            .expect("children")
            .push(json!(8));
    });
    assert_names_field(&error, "payload.dom[8].parent");
    assert!(error.to_string().contains("cycle"), "{error}");
}

#[test]
fn duplicate_ids_name_the_repeat() {
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["dom"][5]["id"] = json!(4);
    });
    assert_names_field(&error, "payload.dom[5].id");
    let error = adapt_error("sign-in.json", |value| value["spans"][3]["id"] = json!(2));
    insta::assert_snapshot!(error, @"invalid snapshot at `spans[3].id`: repeats 2, which index 2 already holds");
    let error = adapt_error("sign-in.json", |value| {
        let source = value["sources"][0].clone();
        value["sources"]
            .as_array_mut()
            .expect("sources")
            .push(source);
    });
    assert_names_field(&error, "sources[1].id");
    let error = adapt_error("sign-in.json", |value| {
        let first = value["payload"]["ax"][2]["nodeId"].clone();
        value["payload"]["ax"][3]["nodeId"] = first;
    });
    assert_names_field(&error, "payload.ax[3].nodeId");
}

#[test]
fn dangling_ax_forced_node_names_the_record() {
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["ax_forced"][0]["node"] = json!(999);
    });
    assert_names_field(&error, "payload.ax_forced[0].node");
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["ax_forced"][0]["backend_id"] = json!(999);
    });
    assert_names_field(&error, "payload.ax_forced[0].backend_id");
}

#[test]
fn dangling_diagnostic_node_names_the_diagnostic() {
    let error = adapt_error("sign-in.json", |value| {
        value["diagnostics"] = json!([{"code": "asset-not-found", "message": "x", "node": 999}]);
    });
    assert_names_field(&error, "diagnostics[0].node");
}

#[test]
fn dangling_ax_references_name_the_field() {
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["ax"][0]["childIds"][0] = json!("missing");
    });
    insta::assert_snapshot!(error, @r#"invalid snapshot at `payload.ax[0].childIds[0]`: names node "missing", which is not in `payload.ax`"#);
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["ax"][3]["parentId"] = json!("missing");
    });
    assert_names_field(&error, "payload.ax[3].parentId");
}

#[test]
fn second_ax_root_names_its_parent_id() {
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["ax"][3]
            .as_object_mut()
            .expect("an ax node")
            .remove("parentId");
    });
    assert_names_field(&error, "payload.ax[3].parentId");
}

#[test]
fn ax_node_reached_twice_names_the_second_child_entry() {
    let error = adapt_error("sign-in.json", |value| {
        let children = value["payload"]["ax"][0]["childIds"]
            .as_array_mut()
            .expect("child ids");
        let first = children[0].clone();
        children.push(first);
    });
    let children = golden_value(Path::new("sign-in.json"))["payload"]["ax"][0]["childIds"]
        .as_array()
        .expect("child ids")
        .len();
    assert_names_field(&error, &format!("payload.ax[0].childIds[{children}]"));
}

#[test]
fn dangling_focus_step_names_the_step() {
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["focus"]["steps"][0]["node"] = json!(999);
    });
    assert_names_field(&error, "payload.focus.steps[0].node");
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["focus"]["steps"][0]["backend_id"] = json!(999);
    });
    assert_names_field(&error, "payload.focus.steps[0].backend_id");
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["focus"]["cycle_start"] = json!(99);
    });
    insta::assert_snapshot!(error, @"invalid snapshot at `payload.focus.cycle_start`: is 99, outside the 5 steps of the walk");
}

#[test]
fn span_past_its_source_length_names_the_range() {
    let length = std::fs::metadata(fixtures().join("pages/sign-in.html"))
        .expect("the page exists")
        .len();
    let error = adapt_error("sign-in.json", |value| {
        value["spans"][1]["start_tag"]["end"] = json!(length + 1);
    });
    assert_names_field(&error, "spans[1].start_tag");
    let error = adapt_error("sign-in.json", |value| {
        value["spans"][1]["element"]["end"] = json!(length + 1);
    });
    assert_names_field(&error, "spans[1].element");
}

#[test]
fn span_ending_before_it_starts_is_rejected_without_a_source_length() {
    let name = Path::new("sign-in.json");
    let mut value = golden_value(name);
    value["spans"][1]["start_tag"]["start"] = json!(500);
    value["spans"][1]["start_tag"]["end"] = json!(400);
    let snapshot = read_value(name, &value);
    let error = adapt(&snapshot, &BTreeMap::new()).expect_err("a reversed range is rejected");
    insta::assert_snapshot!(error, @"invalid snapshot at `spans[1].start_tag`: starts at byte 500, after its end 400");
}

#[test]
fn malformed_dom_snapshot_names_its_path() {
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["dom_snapshot"]["documents"][0]["layout"]["nodeIndex"][0] = json!(99_999);
    });
    assert_names_field(
        &error,
        "payload.dom_snapshot.documents[0].layout.nodeIndex[0]",
    );
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["dom_snapshot"]["documents"][0]["layout"]["styles"][1][1] = json!(99_999);
    });
    assert_names_field(
        &error,
        "payload.dom_snapshot.documents[0].layout.styles[1][1]",
    );
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["dom_snapshot"]
            .as_object_mut()
            .expect("a DOM snapshot")
            .remove("documents");
    });
    insta::assert_snapshot!(error, @"invalid snapshot at `payload.dom_snapshot.documents`: expected an array, found nothing");
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["dom_snapshot"]["strings"][0] = json!(7);
    });
    assert_names_field(&error, "payload.dom_snapshot.strings[0]");
}

#[test]
fn known_property_of_an_unexpected_type_names_its_value() {
    let name = Path::new("sign-in.json");
    let value = golden_value(name);
    let textbox = ax_position(&value, |node| node["role"]["value"] == json!("textbox"));
    let focusable = value["payload"]["ax"][textbox]["properties"]
        .as_array()
        .expect("properties")
        .iter()
        .position(|property| property["name"] == json!("focusable"))
        .expect("a focusable property");
    let error = adapt_error("sign-in.json", |value| {
        value["payload"]["ax"][textbox]["properties"][focusable]["value"]["value"] = json!("yes");
    });
    assert_eq!(
        error.to_string(),
        format!(
            "invalid snapshot at `payload.ax[{textbox}].properties[{focusable}].value`: expected a boolean for `focusable`, found \"yes\""
        )
    );
}

const DEPTH: u32 = 100_000;

/// A snapshot of sign-in's envelope whose DOM is a document holding a chain of `DEPTH` nested
/// elements and whose accessibility tree is `ax`.
fn chain_snapshot(ax: Vec<Value>, ax_forced: Vec<Value>, dom_snapshot: Option<Value>) -> Snapshot {
    let mut value = golden_value(Path::new("sign-in.json"));
    value["spans"] = json!([]);
    value["diagnostics"] = json!([]);
    let mut dom = vec![json!({
        "id": 0, "backend_id": 1, "children": [1], "kind": "document", "attributes": []
    })];
    for id in 1..=DEPTH {
        let children = if id == DEPTH {
            json!([])
        } else {
            json!([id + 1])
        };
        dom.push(json!({
            "id": id, "backend_id": id + 1, "parent": id - 1, "children": children,
            "kind": "element", "tag": "div", "attributes": [], "origin": "script"
        }));
    }
    let payload = value["payload"].as_object_mut().expect("a payload");
    payload.insert("dom".to_owned(), Value::Array(dom));
    payload.insert("ax".to_owned(), Value::Array(ax));
    payload.insert("ax_forced".to_owned(), Value::Array(ax_forced));
    payload.insert("focus".to_owned(), json!({"steps": [], "truncated": false}));
    match dom_snapshot {
        Some(dom_snapshot) => payload.insert("dom_snapshot".to_owned(), dom_snapshot),
        None => payload.remove("dom_snapshot"),
    };
    read_value(Path::new("chain"), &value)
}

fn root_ax(children: &[&str]) -> Value {
    json!({
        "nodeId": "0", "ignored": false, "role": {"type": "internalRole", "value": "RootWebArea"},
        "childIds": children, "backendDOMNodeId": 1
    })
}

#[test]
fn deep_ax_chain_adapts_without_overflowing_the_stack() {
    let mut ax = vec![root_ax(&["1"])];
    for id in 1..=DEPTH {
        let children = if id == DEPTH {
            json!([])
        } else {
            json!([(id + 1).to_string()])
        };
        ax.push(json!({
            "nodeId": id.to_string(), "ignored": false, "role": {"type": "role", "value": "generic"},
            "parentId": (id - 1).to_string(), "childIds": children, "backendDOMNodeId": id + 1
        }));
    }
    let snapshot = chain_snapshot(ax, Vec::new(), None);
    let model = adapt(&snapshot, &BTreeMap::new()).expect("the chain adapts");
    assert_eq!(model.nodes.len(), usize::try_from(DEPTH).expect("fits") + 1);
    assert_eq!(model.excluded, []);
    let deepest = model
        .nodes
        .last()
        .and_then(ExposedNode::as_accessible)
        .expect("a node");
    assert_eq!(deepest.dom.as_ref().map(|dom| dom.dom_id), Some(DEPTH));
}

#[test]
fn deep_dom_chain_adapts_without_overflowing_the_stack() {
    // Only the document and the first element have layout nodes, so every deeper element is
    // hidden by the second element, the topmost without one.
    let forced: Vec<Value> = (1..=DEPTH)
        .map(|id| {
            json!({"node": id, "backend_id": id + 1, "nodes": [{
                "nodeId": format!("forced-{id}"), "ignored": true,
                "ignoredReasons": [{"name": "notRendered", "value": {"type": "boolean", "value": true}}],
                "role": {"type": "role", "value": "none"}, "backendDOMNodeId": id + 1
            }]})
        })
        .collect();
    let backend_ids: Vec<u32> = (1..=DEPTH + 1).collect();
    let dom_snapshot = json!({
        "strings": ["block", "visible"],
        "documents": [{
            "nodes": {"backendNodeId": backend_ids},
            "layout": {"nodeIndex": [0, 1], "styles": [[], [0, 1]]}
        }]
    });
    let snapshot = chain_snapshot(vec![root_ax(&[])], forced, Some(dom_snapshot));
    let model = adapt(&snapshot, &BTreeMap::new()).expect("the chain adapts");
    assert_eq!(model.excluded.len(), usize::try_from(DEPTH).expect("fits"));
    let deepest = model.excluded.last().expect("an excluded node");
    let ancestor = deepest.reasons[0]
        .hiding_ancestor
        .as_ref()
        .expect("a hiding ancestor");
    assert_eq!(ancestor.value.dom_id, 2);
    assert_eq!(deepest.source, SourceClass::Script);
    assert_eq!(model.excluded[1].reasons[0].hiding_ancestor, None);
}

#[test]
fn deep_dom_cycle_is_an_error_not_a_hang() {
    let snapshot = chain_snapshot(vec![root_ax(&[])], Vec::new(), None);
    let mut snapshot = snapshot;
    let Payload::Chromium(payload) = &mut snapshot.payload else {
        panic!("a Chromium payload");
    };
    // The deepest element lists the first as its child, closing a 100,000-node loop.
    payload.dom[0].children.clear();
    payload.dom[1].parent = Some(DEPTH);
    payload.dom.last_mut().expect("a node").children.push(1);
    let error = adapt(&snapshot, &BTreeMap::new()).expect_err("a cycle is rejected");
    assert_names_field(&error, "payload.dom[1].parent");
}
