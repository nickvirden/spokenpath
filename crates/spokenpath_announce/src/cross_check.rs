//! The cross-check of a transcript's roles and names against what Safari answers for the same
//! page.
//!
//! Only items that are accessible nodes located by a span are compared; text runs, the document
//! root and implied, script or unknown-origin nodes get no cross-check, and neither do the
//! elements the transcript's engine excludes (ADR-0009 Decision 1). Role and name are compared
//! separately, as exact strings with no synonym mapping, case folding or whitespace normalisation,
//! and an absent role or name compares as `""`: differences are reported, not corrected. A field
//! is `not-checked`, never `same`, when Safari answered that call with an error, when Safari holds
//! no element with the span, or when more than one item or more than one Safari element has the
//! span. No outcome says which engine is right.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Serialize;
use spokenpath_model::safari_webdriver::{Answer, ElementAnswers, SafariAnswers, SpanAnswers};
use spokenpath_model::{Engine, ExposedNode, Model, SourceClass};

use crate::transcript::{Item, LayerA, Transcript};

/// The engine a transcript was cross-checked against and what the comparison found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct CrossCheck {
    /// The engine token of the cross-check snapshot, `safari-webdriver`.
    pub engine: &'static str,
    /// Safari's version, as the browser reports it.
    pub engine_version: String,
    /// Safari's build number, when the capture recorded it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safari_build: Option<String>,
    /// The macOS product version, when the capture recorded it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macos_version: Option<String>,
    /// The macOS build version, when the capture recorded it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macos_build: Option<String>,
    /// The number of items with a cross-check.
    pub compared: usize,
    /// The number of items with at least one field that differs.
    pub differs: usize,
    /// The number of items with at least one field that is not checked.
    pub not_checked: usize,
}

/// The cross-check of one item's role and name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ItemCrossCheck {
    /// The role, compared with Safari's computed role.
    pub role: FieldCheck,
    /// The name, compared with Safari's computed label.
    pub name: FieldCheck,
}

impl ItemCrossCheck {
    fn both(field: &FieldCheck) -> Self {
        ItemCrossCheck {
            role: field.clone(),
            name: field.clone(),
        }
    }

    /// The fields in printing order, each with its name.
    #[must_use]
    pub fn fields(&self) -> [(&'static str, &FieldCheck); 2] {
        [("role", &self.role), ("name", &self.name)]
    }
}

/// What came of comparing one field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum FieldCheck {
    /// Both engines expose the same string.
    Same {
        /// Safari's string, raw.
        safari: String,
        /// The transcript engine's string, raw.
        chromium: String,
    },
    /// The engines expose different strings.
    Differs {
        /// Safari's string, raw.
        safari: String,
        /// The transcript engine's string, raw.
        chromium: String,
    },
    /// The field could not be compared.
    NotChecked {
        /// Why: the error Safari answered with, or a reason.
        error: NotChecked,
    },
}

impl FieldCheck {
    fn compare(safari: &str, chromium: &str) -> Self {
        let (safari, chromium) = (safari.to_owned(), chromium.to_owned());
        if safari == chromium {
            FieldCheck::Same { safari, chromium }
        } else {
            FieldCheck::Differs { safari, chromium }
        }
    }

    fn of(answer: &Answer, chromium: &str) -> Self {
        match answer {
            Answer::Value(safari) => FieldCheck::compare(safari, chromium),
            Answer::Error(error) => FieldCheck::NotChecked {
                error: NotChecked::Engine(EngineErrorRecord {
                    error: error.code.clone(),
                    message: error.message.clone(),
                }),
            },
        }
    }

    fn not_checked(reason: NotCheckedReason) -> Self {
        FieldCheck::NotChecked {
            error: NotChecked::Reason(reason),
        }
    }
}

/// Why a field was not checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum NotChecked {
    /// Safari answered the call with this error.
    Engine(EngineErrorRecord),
    /// Safari's answer cannot be paired with the item.
    Reason(NotCheckedReason),
}

/// An error Safari answered a call with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct EngineErrorRecord {
    /// The `WebDriver` error code, such as `unknown error`.
    pub error: String,
    /// The message, possibly empty.
    pub message: String,
}

/// Why Safari's answer cannot be paired with an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NotCheckedReason {
    /// Safari's DOM holds no element with the item's span, such as an element inside a shadow
    /// root, which `WebDriver` does not search.
    NoSafariElement,
    /// More than one item, or more than one Safari element, has the span.
    SpanNotUnique,
}

/// Fills `transcript`'s cross-check fields from Safari's `answers` for the same page. `model` is
/// the model `transcript` was announced from; it gives each item's span.
pub fn cross_check(transcript: &mut Transcript, model: &Model, answers: &SafariAnswers) {
    let span_by_dom: BTreeMap<u32, u32> = model
        .nodes
        .iter()
        .filter_map(ExposedNode::as_accessible)
        .filter_map(|node| match (&node.dom, &node.source) {
            (Some(dom), SourceClass::Span(span)) => Some((dom.dom_id, span.span)),
            _ => None,
        })
        .collect();
    let spans: Vec<Option<u32>> = transcript
        .nodes
        .iter()
        .map(|item| compared_span(item, &span_by_dom))
        .collect();
    let mut items_per_span: BTreeMap<u32, usize> = BTreeMap::new();
    for span in spans.iter().flatten() {
        *items_per_span.entry(*span).or_default() += 1;
    }

    let mut summary = CrossCheck {
        engine: Engine::SafariWebdriver.as_str(),
        engine_version: answers.engine_version.clone(),
        safari_build: answers.safari_build.clone(),
        macos_version: answers.macos_version.clone(),
        macos_build: answers.macos_build.clone(),
        compared: 0,
        differs: 0,
        not_checked: 0,
    };
    for (item, span) in transcript.nodes.iter_mut().zip(spans) {
        let (Some(span), LayerA::Accessible(record)) = (span, &item.layer_a) else {
            continue;
        };
        let check = if items_per_span.get(&span).copied().unwrap_or_default() > 1 {
            ItemCrossCheck::both(&FieldCheck::not_checked(NotCheckedReason::SpanNotUnique))
        } else {
            match answers.by_span.get(&span) {
                None => ItemCrossCheck::both(&FieldCheck::not_checked(
                    NotCheckedReason::NoSafariElement,
                )),
                Some(SpanAnswers::Several) => {
                    ItemCrossCheck::both(&FieldCheck::not_checked(NotCheckedReason::SpanNotUnique))
                }
                Some(SpanAnswers::One(ElementAnswers { role, label })) => ItemCrossCheck {
                    role: FieldCheck::of(role, record.role.as_ref().map_or("", |role| &role.token)),
                    name: FieldCheck::of(label, record.name.text.as_deref().unwrap_or_default()),
                },
            }
        };
        summary.compared += 1;
        let fields = check.fields();
        if fields
            .iter()
            .any(|(_, field)| matches!(field, FieldCheck::Differs { .. }))
        {
            summary.differs += 1;
        }
        if fields
            .iter()
            .any(|(_, field)| matches!(field, FieldCheck::NotChecked { .. }))
        {
            summary.not_checked += 1;
        }
        item.cross_check = Some(check);
    }
    transcript.cross_check = Some(summary);
}

/// The span an item is compared under: an accessible item located by a span, found through its
/// DOM node.
fn compared_span(item: &Item, span_by_dom: &BTreeMap<u32, u32>) -> Option<u32> {
    let LayerA::Accessible(record) = &item.layer_a else {
        return None;
    };
    if record.source.class != "span" {
        return None;
    }
    span_by_dom.get(&record.dom.as_ref()?.dom_id).copied()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use spokenpath_model::Model;
    use spokenpath_model::chromium::adapt;
    use spokenpath_model::safari_webdriver::{
        Answer, ElementAnswers, EngineError, SafariAnswers, SpanAnswers,
    };
    use spokenpath_snapshot::read;

    use super::{FieldCheck, NotChecked, NotCheckedReason, cross_check};
    use crate::{EngineErrorRecord, LayerA, Options, Transcript, announce};

    /// The model of `chromium-basic.json`, whose items 2, 4, 6 and 7 (the heading, the label, the
    /// input and the button) are located by spans 1 to 4, and whose paragraph (span 5) is excluded.
    fn basic() -> Model {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/snapshots/chromium-basic.json");
        let text = std::fs::read_to_string(path).expect("chromium-basic.json is readable");
        let snapshot = read(&text).expect("reads");
        adapt(&snapshot, &BTreeMap::new()).expect("adapts")
    }

    fn value(text: &str) -> Answer {
        Answer::Value(text.to_owned())
    }

    fn one(role: Answer, label: Answer) -> SpanAnswers {
        SpanAnswers::One(ElementAnswers { role, label })
    }

    fn safari(by_span: impl IntoIterator<Item = (u32, SpanAnswers)>) -> SafariAnswers {
        SafariAnswers {
            engine_version: "27.0.1".to_owned(),
            safari_build: None,
            macos_version: Some("27.0.1".to_owned()),
            macos_build: None,
            by_span: by_span.into_iter().collect(),
        }
    }

    fn checked(model: &Model, answers: &SafariAnswers) -> Transcript {
        let mut transcript = announce(model, Options::default());
        cross_check(&mut transcript, model, answers);
        transcript
    }

    fn same(text: &str) -> FieldCheck {
        FieldCheck::Same {
            safari: text.to_owned(),
            chromium: text.to_owned(),
        }
    }

    #[test]
    fn only_span_located_accessible_items_are_compared() {
        let model = basic();
        let answers = safari([
            (1, one(value("heading"), value("Café sign-up"))),
            (2, one(value("LabelText"), value(""))),
            (3, one(value("textbox"), value("Email"))),
            (4, one(value("button"), value("Join"))),
            (5, one(value("paragraph"), value("Décor"))),
        ]);
        let transcript = checked(&model, &answers);
        let compared: Vec<usize> = transcript
            .nodes
            .iter()
            .filter(|item| item.cross_check.is_some())
            .map(|item| item.number)
            .collect();
        // The root, the text runs and the excluded paragraph get no cross-check.
        assert_eq!(compared, [2, 4, 6, 7]);
        for item in &transcript.nodes {
            if matches!(item.layer_a, LayerA::TextRun(_)) {
                assert_eq!(item.cross_check, None, "item {}", item.number);
            }
        }
        let summary = transcript.cross_check.expect("a summary");
        assert_eq!(
            (summary.compared, summary.differs, summary.not_checked),
            (4, 0, 0)
        );
        assert_eq!(summary.engine, "safari-webdriver");
        // An absent name compares as an empty string.
        let label = transcript.nodes[3].cross_check.as_ref().expect("checked");
        assert_eq!(label.name, same(""));
    }

    #[test]
    fn strings_are_compared_exactly_and_each_field_on_its_own() {
        let model = basic();
        let error = EngineError {
            code: "unknown error".to_owned(),
            message: String::new(),
        };
        let answers = safari([
            (1, one(value("Heading"), value("Café sign-up "))),
            (3, one(Answer::Error(error), value("Email"))),
        ]);
        let transcript = checked(&model, &answers);
        let heading = transcript.nodes[1].cross_check.as_ref().expect("checked");
        assert_eq!(
            heading.role,
            FieldCheck::Differs {
                safari: "Heading".to_owned(),
                chromium: "heading".to_owned(),
            }
        );
        assert!(matches!(heading.name, FieldCheck::Differs { .. }));
        let input = transcript.nodes[5].cross_check.as_ref().expect("checked");
        assert_eq!(
            input.role,
            FieldCheck::NotChecked {
                error: NotChecked::Engine(EngineErrorRecord {
                    error: "unknown error".to_owned(),
                    message: String::new(),
                }),
            }
        );
        assert_eq!(input.name, same("Email"));
        let summary = transcript.cross_check.expect("a summary");
        // The label and the button have no Safari element.
        assert_eq!(
            (summary.compared, summary.differs, summary.not_checked),
            (4, 1, 3)
        );
    }

    #[test]
    fn a_span_without_a_single_owner_is_never_same() {
        let model = basic();
        let answers = safari([
            (1, SpanAnswers::Several),
            (4, one(value("button"), value("Join"))),
        ]);
        let mut transcript = announce(&model, Options::default());
        // A second item located by the button's span.
        let button = transcript.nodes[6].clone();
        transcript.nodes.push(button);
        cross_check(&mut transcript, &model, &answers);
        let not_unique = FieldCheck::NotChecked {
            error: NotChecked::Reason(NotCheckedReason::SpanNotUnique),
        };
        for index in [1, 6, 9] {
            let check = transcript.nodes[index]
                .cross_check
                .as_ref()
                .expect("checked");
            assert_eq!(check.role, not_unique, "item {}", index + 1);
            assert_eq!(check.name, not_unique, "item {}", index + 1);
        }
        let missing = transcript.nodes[3].cross_check.as_ref().expect("checked");
        assert_eq!(
            missing.role,
            FieldCheck::NotChecked {
                error: NotChecked::Reason(NotCheckedReason::NoSafariElement),
            }
        );
    }

    #[test]
    fn a_transcript_without_a_cross_check_serialises_no_cross_check_field() {
        let transcript = announce(&basic(), Options::default());
        let json = serde_json::to_string(&transcript).expect("serialises");
        assert!(!json.contains("cross_check"), "{json}");
    }
}
