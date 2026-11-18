//! Layer B: one illustrative phrase per item, approximate, from a fixed template table.

use std::borrow::Cow;

use serde::Serialize;

use crate::transcript::{
    AccessibleRecord, LayerA, NameStatus, SetPositionRecord, StatesRecord, TristateToken,
};

/// One Layer B template: a stable identifier and the pattern it renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Template {
    /// The stable kebab-case identifier stored with every phrase.
    pub id: &'static str,
    /// The pattern. `{parts}` is the comma-and-space-joined list of the present parts; a
    /// bracketed segment appears only when there are parts.
    pub pattern: &'static str,
}

/// A text run: its text.
pub const TEXT_RUN: Template = Template {
    id: "text-run",
    pattern: "{text}",
};

/// A named item with a spoken role.
pub const NAMED: Template = Template {
    id: "named",
    pattern: "{name}, {role}[, {parts}]",
};

/// An unnamed item with a spoken role.
pub const UNNAMED: Template = Template {
    id: "unnamed",
    pattern: "{role}[, {parts}]",
};

/// A named item whose role is engine-internal, `generic`, or absent.
pub const NAMED_UNSPOKEN_ROLE: Template = Template {
    id: "named-unspoken-role",
    pattern: "{name}[, {parts}]",
};

/// An unnamed item whose role is engine-internal, `generic`, or absent; possibly empty.
pub const UNNAMED_UNSPOKEN_ROLE: Template = Template {
    id: "unnamed-unspoken-role",
    pattern: "{parts}",
};

/// Every Layer B template.
pub const TEMPLATES: [Template; 5] = [
    TEXT_RUN,
    NAMED,
    UNNAMED,
    NAMED_UNSPOKEN_ROLE,
    UNNAMED_UNSPOKEN_ROLE,
];

/// The illustrative phrase of one item, approximate, with the template that rendered it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Approx {
    /// The template's identifier.
    pub template: &'static str,
    /// The phrase, escaped for a terminal.
    pub phrase: String,
}

/// Renders the phrase of an item from its Layer A record alone, so it can hold nothing the
/// record does not.
pub(crate) fn render(layer_a: &LayerA) -> Approx {
    let (template, phrase) = match layer_a {
        LayerA::TextRun(run) => (TEXT_RUN, run.text.clone()),
        LayerA::Accessible(record) => accessible(record),
    };
    Approx {
        template: template.id,
        phrase: escape_for_terminal(&phrase).into_owned(),
    }
}

fn accessible(record: &AccessibleRecord) -> (Template, String) {
    let parts = parts(record);
    let spoken_role = record
        .role
        .as_ref()
        .and_then(|role| role.aria())
        .filter(|&role| role != "generic");
    let name = match record.name.status {
        NameStatus::Named => record.name.text.as_deref(),
        NameStatus::NoAccessibleName => None,
    };
    let (template, mut words) = match (name, spoken_role) {
        (Some(name), Some(role)) => (NAMED, vec![name, role]),
        (None, Some(role)) => (UNNAMED, vec![role]),
        (Some(name), None) => (NAMED_UNSPOKEN_ROLE, vec![name]),
        (None, None) => (UNNAMED_UNSPOKEN_ROLE, Vec::new()),
    };
    words.extend(parts.iter().map(String::as_str));
    (template, words.join(", "))
}

/// The present parts, in order: level, value, state words, set position.
fn parts(record: &AccessibleRecord) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(level) = record.states.level {
        parts.push(format!("level {level}"));
    }
    if let Some(value) = record.value.as_deref().filter(|value| !value.is_empty()) {
        parts.push(value.to_owned());
    }
    parts.extend(state_words(&record.states).into_iter().map(str::to_owned));
    if let Some(position) = &record.set_position {
        parts.push(set_position(position.value));
    }
    parts
}

fn state_words(states: &StatesRecord) -> Vec<&'static str> {
    let mut words = Vec::new();
    if let Some(checked) = states.checked {
        words.push(match checked {
            TristateToken::True => "checked",
            TristateToken::False => "not checked",
            TristateToken::Mixed => "mixed",
        });
    }
    if let Some(pressed) = states.pressed {
        words.push(match pressed {
            TristateToken::True => "pressed",
            TristateToken::False => "not pressed",
            TristateToken::Mixed => "mixed",
        });
    }
    if let Some(expanded) = states.expanded {
        words.push(if expanded { "expanded" } else { "collapsed" });
    }
    for (state, word) in [
        (states.selected, "selected"),
        (states.disabled, "disabled"),
        (states.required, "required"),
        (states.readonly, "read-only"),
    ] {
        if state == Some(true) {
            words.push(word);
        }
    }
    if states
        .invalid
        .as_deref()
        .is_some_and(|token| token != "false")
    {
        words.push("invalid");
    }
    words
}

fn set_position(position: SetPositionRecord) -> String {
    match position.size {
        Some(size) => format!("{} of {size}", position.position),
        None => position.position.to_string(),
    }
}

/// Whether a terminal must not receive `character` raw: a control character (C0, DEL, or C1,
/// which includes the CSI introducer U+009B); a bidirectional embedding, override or isolate
/// control, or an implicit direction mark (U+200E, U+200F, U+061C), which reorder what a terminal
/// shows; a line or paragraph separator (U+2028, U+2029), which can break a line; or a zero-width
/// character (U+200B to U+200D, U+2060, U+FEFF), which hides text or makes two strings look equal.
fn needs_escape(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{202A}'..='\u{202E}'
                | '\u{2066}'..='\u{2069}'
                | '\u{200E}'
                | '\u{200F}'
                | '\u{061C}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{200B}'..='\u{200D}'
                | '\u{2060}'
                | '\u{FEFF}'
        )
}

/// `text` with each control character, bidirectional control or mark, line or paragraph
/// separator, and zero-width character replaced by its `\u{..}` escape,
/// as [`char::escape_unicode`] writes it; every other character passes unchanged. Borrows `text`
/// when nothing needs escaping.
#[must_use]
pub fn escape_for_terminal(text: &str) -> Cow<'_, str> {
    if !text.chars().any(needs_escape) {
        return Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len() + 8);
    for character in text.chars() {
        if needs_escape(character) {
            escaped.extend(character.escape_unicode());
        } else {
            escaped.push(character);
        }
    }
    Cow::Owned(escaped)
}
