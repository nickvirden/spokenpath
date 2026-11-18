use std::fmt;

use spokenpath_snapshot::Engine;

/// Why an adapter could not build a [`Model`](crate::Model) from a snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdaptError {
    /// The snapshot's payload is for an engine this adapter does not read.
    UnsupportedEngine {
        /// The engine the adapter reads.
        expected: Engine,
        /// The engine the snapshot's payload is for.
        found: Engine,
    },
    /// A field holds a reference that resolves to nothing, a value that contradicts another
    /// field, or a value of an unexpected type. `path` is dotted, with indexes, in the style of
    /// `spokenpath_snapshot::ReadError`, such as `payload.dom[4].children[1]`; `message` states
    /// the value that failed.
    Field {
        /// Where the failing field is, such as `spans[2].source`.
        path: String,
        /// What is wrong with it, including the failing value.
        message: String,
    },
}

/// The most characters of an input value that an error message repeats, so a hostile snapshot
/// cannot make a message as large as itself.
const EXCERPT_CHARS: usize = 80;

/// `value` as text, cut to [`EXCERPT_CHARS`] characters with `…` marking the cut.
pub(crate) fn excerpt(value: &impl fmt::Display) -> String {
    let text = value.to_string();
    match text.char_indices().nth(EXCERPT_CHARS) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text,
    }
}

impl AdaptError {
    pub(crate) fn field(path: impl Into<String>, message: impl Into<String>) -> Self {
        AdaptError::Field {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for AdaptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdaptError::UnsupportedEngine { expected, found } => write!(
                formatter,
                "the {expected} adapter cannot read a snapshot whose payload is for `{found}`"
            ),
            AdaptError::Field { path, message } => {
                write!(formatter, "invalid snapshot at `{path}`: {message}")
            }
        }
    }
}

impl std::error::Error for AdaptError {}
