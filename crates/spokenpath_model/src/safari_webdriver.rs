//! The reader for Safari `WebDriver` snapshots: Safari's computed role and label for each span,
//! with the Safari and macOS versions that answered.
//!
//! The reader turns the payload into engine-neutral answers keyed by span id. It neither compares
//! nor corrects them: a cross-check against another engine's model is the announce crate's work
//! (ADR-0009: differences are reported, not corrected).

use std::collections::BTreeMap;

use spokenpath_snapshot::{Engine, Payload, Snapshot, WebdriverAnswer};

use crate::AdaptError;

/// What Safari answered for a snapshot's spans, and the versions that answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafariAnswers {
    /// Safari's version, as the browser reports it.
    pub engine_version: String,
    /// Safari's build number, when the capture recorded it.
    pub safari_build: Option<String>,
    /// The macOS product version, when the capture recorded it.
    pub macos_version: Option<String>,
    /// The macOS build version, when the capture recorded it.
    pub macos_build: Option<String>,
    /// The answers by span id. A span with no element in Safari's DOM has no entry.
    pub by_span: BTreeMap<u32, SpanAnswers>,
}

/// Safari's answers for the elements that carry one span id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpanAnswers {
    /// Exactly one element carries the span.
    One(ElementAnswers),
    /// More than one element carries the span, so no answer belongs to it alone.
    Several,
}

/// Safari's role and label for one element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementAnswers {
    /// The computed role.
    pub role: Answer,
    /// The computed label: the accessible name.
    pub label: Answer,
}

/// One answer: a value, possibly empty, or the error the browser answered with instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The value the browser returned.
    Value(String),
    /// The browser returned an error instead of a value.
    Error(EngineError),
}

/// An error the browser returned for one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineError {
    /// The error code, such as `unknown error`.
    pub code: String,
    /// The message, possibly empty.
    pub message: String,
}

/// Reads Safari's answers from a `safari-webdriver` snapshot.
///
/// # Errors
///
/// Returns [`AdaptError::UnsupportedEngine`] when the snapshot's payload is for another engine,
/// and [`AdaptError::Field`] for an element that holds both or neither of an answer and its
/// error, which a snapshot from `spokenpath_snapshot::read` never has.
pub fn answers(snapshot: &Snapshot) -> Result<SafariAnswers, AdaptError> {
    let Payload::SafariWebdriver(payload) = &snapshot.payload else {
        return Err(AdaptError::UnsupportedEngine {
            expected: Engine::SafariWebdriver,
            found: snapshot.payload.engine(),
        });
    };
    let mut by_span: BTreeMap<u32, SpanAnswers> = BTreeMap::new();
    for (index, element) in payload.elements.iter().enumerate() {
        let field = |name: &str| {
            AdaptError::field(
                format!("payload.elements[{index}]"),
                format!("expected exactly one of `{name}` and `{name}_error`"),
            )
        };
        let role = element.role_answer().ok_or_else(|| field("role"))?;
        let label = element.label_answer().ok_or_else(|| field("label"))?;
        let answers = ElementAnswers {
            role: answer(role),
            label: answer(label),
        };
        by_span
            .entry(element.span)
            .and_modify(|existing| *existing = SpanAnswers::Several)
            .or_insert(SpanAnswers::One(answers));
    }
    Ok(SafariAnswers {
        engine_version: snapshot.capture.engine_version.clone(),
        safari_build: payload.safari_build.clone(),
        macos_version: payload.macos_version.clone(),
        macos_build: payload.macos_build.clone(),
        by_span,
    })
}

fn answer(answer: WebdriverAnswer<'_>) -> Answer {
    match answer {
        WebdriverAnswer::Value(value) => Answer::Value(value.to_owned()),
        WebdriverAnswer::Error(error) => Answer::Error(EngineError {
            code: error.error.clone(),
            message: error.message.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::{Value, json};
    use spokenpath_snapshot::{Engine, read};

    use super::{Answer, ElementAnswers, EngineError, SpanAnswers, answers};
    use crate::AdaptError;

    fn basic() -> Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/snapshots/chromium-basic.json");
        let text = std::fs::read_to_string(&path).expect("chromium-basic.json is readable");
        serde_json::from_str(&text).expect("the fixture is JSON")
    }

    fn safari(elements: &Value) -> spokenpath_snapshot::Snapshot {
        let mut value = basic();
        value["version"]["minor"] = json!(1);
        value["capture"]["engine"] = json!("safari-webdriver");
        value["capture"]["engine_version"] = json!("27.0.1");
        value["payload"] = json!({
            "engine": "safari-webdriver",
            "elements": elements,
            "macos_version": "27.0.1",
        });
        read(&value.to_string()).expect("the Safari snapshot reads")
    }

    #[test]
    fn answers_are_keyed_by_span_with_errors_kept() {
        let snapshot = safari(&json!([
            { "span": 1, "role": "heading", "label": "Title" },
            {
                "span": 2,
                "role_error": { "error": "unknown error", "message": "no role" },
                "label": "",
            },
        ]));
        let answers = answers(&snapshot).expect("reads");
        assert_eq!(answers.engine_version, "27.0.1");
        assert_eq!(answers.macos_version.as_deref(), Some("27.0.1"));
        assert_eq!(answers.safari_build, None);
        assert_eq!(
            answers.by_span.get(&1),
            Some(&SpanAnswers::One(ElementAnswers {
                role: Answer::Value("heading".to_owned()),
                label: Answer::Value("Title".to_owned()),
            }))
        );
        assert_eq!(
            answers.by_span.get(&2),
            Some(&SpanAnswers::One(ElementAnswers {
                role: Answer::Error(EngineError {
                    code: "unknown error".to_owned(),
                    message: "no role".to_owned(),
                }),
                label: Answer::Value(String::new()),
            }))
        );
        assert_eq!(answers.by_span.get(&3), None);
    }

    #[test]
    fn a_span_two_elements_carry_has_no_single_answer() {
        let snapshot = safari(&json!([
            { "span": 1, "role": "heading", "label": "A" },
            { "span": 1, "role": "heading", "label": "B" },
        ]));
        let answers = answers(&snapshot).expect("reads");
        assert_eq!(answers.by_span.get(&1), Some(&SpanAnswers::Several));
    }

    #[test]
    fn a_minor_zero_envelope_has_no_answers() {
        let mut value = basic();
        value["capture"]["engine"] = json!("safari-webdriver");
        value["payload"] = json!({ "engine": "safari-webdriver" });
        let snapshot = read(&value.to_string()).expect("reads");
        let answers = answers(&snapshot).expect("reads");
        assert!(answers.by_span.is_empty());
        assert_eq!(answers.macos_build, None);
    }

    #[test]
    fn a_chromium_snapshot_is_refused_naming_its_engine() {
        let snapshot = read(&basic().to_string()).expect("reads");
        let error = answers(&snapshot).expect_err("a Chromium payload is refused");
        assert_eq!(
            error,
            AdaptError::UnsupportedEngine {
                expected: Engine::SafariWebdriver,
                found: Engine::Chromium,
            }
        );
        insta::assert_snapshot!(error, @"the safari-webdriver adapter cannot read a snapshot whose payload is for `chromium`");
    }
}
