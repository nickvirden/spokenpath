//! The tab order: the focus walk step by step, and how it ended.

use schemars::JsonSchema;
use serde::Serialize;
use spokenpath_model::{FocusStop, FocusWalk, NodeIndex};

use crate::transcript::{DomRecord, DomSources, Marked, Marking, SourceRecord, dom_record};

/// The focus walk by repeated Tab presses, as captured, and how it ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TabOrder {
    /// One entry per Tab press, in order.
    pub steps: Vec<TabStep>,
    /// How the walk ended, computed by tool.
    pub ending: Marked<Ending>,
}

/// Where focus was after one Tab press.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TabStep {
    /// The 1-based step number.
    pub step: usize,
    /// The stop.
    pub stop: Stop,
    /// For the step that repeats an earlier one, the step number it repeats.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeats: Option<usize>,
}

/// One stop of the focus walk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Stop {
    /// An element of the snapshot's DOM.
    Element {
        /// The focused element.
        dom: DomRecord,
        /// Where in the page's sources the element came from, when the model holds it.
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<SourceRecord>,
        /// The number of the element's item, when the element is exposed.
        #[serde(skip_serializing_if = "Option::is_none")]
        item: Option<usize>,
    },
    /// Focus left the page.
    LeftPage,
    /// An element the snapshot's DOM does not hold.
    Unresolved {
        /// The engine's id for the node.
        backend_id: u64,
    },
}

/// How a focus walk ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Ending {
    /// The kind of ending.
    pub kind: EndingKind,
    /// One plain line stating what the Tab presses did, from [`ENDING_WORDING`].
    pub wording: String,
}

/// The kind of a focus walk's ending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum EndingKind {
    /// The walk stopped at its step limit.
    Truncated,
    /// Focus left the page, then Tab returned to an earlier stop.
    Wrap,
    /// Tab returned to an earlier stop without focus leaving the page.
    Trap,
    /// Focus left the page and did not return.
    LeftPage,
    /// The walk ended otherwise, including a walk with no steps.
    Ended,
}

/// The wording of one ending kind. `{step}` is the step number Tab returned to; `{count}` is a
/// number with `unit` in its singular or plural form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndingWording {
    /// The ending kind it words.
    pub kind: EndingKind,
    /// The line, with its placeholders.
    pub pattern: &'static str,
    /// The singular and plural unit that `{count}` counts; empty when the pattern has no count.
    pub unit: (&'static str, &'static str),
    /// The line in place of `pattern` when the count is zero, for an ending where "after 0" reads
    /// as a count of something that happened; absent when `pattern` serves every count.
    pub zero_pattern: Option<&'static str>,
}

/// The wording of every ending kind.
pub const ENDING_WORDING: [EndingWording; 5] = [
    EndingWording {
        kind: EndingKind::Truncated,
        pattern: "The walk stopped at its limit after {count}. Later stops are not listed.",
        unit: ("Tab press", "Tab presses"),
        zero_pattern: None,
    },
    EndingWording {
        kind: EndingKind::Wrap,
        pattern: "Focus left the page and Tab returned to step {step}, the normal end of a walk.",
        unit: ("", ""),
        zero_pattern: None,
    },
    EndingWording {
        kind: EndingKind::Trap,
        pattern: "Tab returned to step {step} without focus leaving the page: a focus trap.",
        unit: ("", ""),
        zero_pattern: None,
    },
    EndingWording {
        kind: EndingKind::LeftPage,
        pattern: "Focus left the page after {count}, with no return.",
        unit: ("element stop", "element stops"),
        zero_pattern: Some("Focus left the page without stopping on any element."),
    },
    EndingWording {
        kind: EndingKind::Ended,
        pattern: "The walk ended after {count}.",
        unit: ("step", "steps"),
        zero_pattern: None,
    },
];

impl EndingKind {
    /// The kind's name, such as `wrap` or `left-page`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            EndingKind::Truncated => "truncated",
            EndingKind::Wrap => "wrap",
            EndingKind::Trap => "trap",
            EndingKind::LeftPage => "left-page",
            EndingKind::Ended => "ended",
        }
    }

    fn wording(self) -> &'static EndingWording {
        match self {
            EndingKind::Truncated => &ENDING_WORDING[0],
            EndingKind::Wrap => &ENDING_WORDING[1],
            EndingKind::Trap => &ENDING_WORDING[2],
            EndingKind::LeftPage => &ENDING_WORDING[3],
            EndingKind::Ended => &ENDING_WORDING[4],
        }
    }
}

fn wording(kind: EndingKind, step: usize, count: usize) -> String {
    let entry = kind.wording();
    let unit = if count == 1 {
        entry.unit.0
    } else {
        entry.unit.1
    };
    let pattern = match entry.zero_pattern {
        Some(zero_pattern) if count == 0 => zero_pattern,
        _ => entry.pattern,
    };
    pattern
        .replace("{step}", &step.to_string())
        .replace("{count}", &format!("{count} {unit}"))
}

/// What identifies a stop when matching a repeat: the DOM id of an element, or the engine's id of
/// an unresolved node. A `left-page` stop repeats nothing.
#[derive(PartialEq, Eq)]
enum Identity {
    Dom(u32),
    Backend(u64),
}

fn identity(stop: &FocusStop) -> Option<Identity> {
    match stop {
        FocusStop::LeftPage => None,
        FocusStop::Element { dom, .. } => Some(Identity::Dom(dom.dom_id)),
        FocusStop::Unresolved { backend_id } => Some(Identity::Backend(*backend_id)),
    }
}

/// The tab order of `walk`. `item_number` maps an exposed node to its item number, when it has
/// one.
pub(crate) fn tab_order(
    walk: &FocusWalk,
    sources: &DomSources<'_>,
    item_number: impl Fn(NodeIndex) -> Option<usize>,
) -> TabOrder {
    // The repeating step is the one the walk marks as its cycle start; it repeats the earliest
    // earlier step with the same identity. Repeats of a frame element on consecutive steps are
    // never the cycle start, so they stay ordinary stops.
    let cycle = walk
        .cycle_start
        .and_then(|start| usize::try_from(start).ok())
        .and_then(|start| {
            let identity = identity(walk.steps.get(start)?)?;
            let repeated = walk.steps[..start]
                .iter()
                .position(|stop| self::identity(stop).as_ref() == Some(&identity))?;
            Some((start, repeated))
        });

    let steps = walk
        .steps
        .iter()
        .enumerate()
        .map(|(index, stop)| TabStep {
            step: index + 1,
            stop: match stop {
                FocusStop::LeftPage => Stop::LeftPage,
                FocusStop::Element { dom, node } => Stop::Element {
                    dom: dom_record(dom),
                    source: sources.source(dom),
                    item: node.and_then(&item_number),
                },
                FocusStop::Unresolved { backend_id } => Stop::Unresolved {
                    backend_id: *backend_id,
                },
            },
            repeats: cycle
                .filter(|&(start, _)| start == index)
                .map(|(_, repeated)| repeated + 1),
        })
        .collect();

    TabOrder {
        steps,
        ending: Marked {
            value: ending(walk, cycle),
            by: Marking::ComputedByTool,
        },
    }
}

fn ending(walk: &FocusWalk, cycle: Option<(usize, usize)>) -> Ending {
    let steps = walk.steps.len();
    let (kind, step, count) = if walk.truncated {
        (EndingKind::Truncated, 0, steps)
    } else if let Some((start, repeated)) = cycle {
        let left_page_before = walk.steps[..start]
            .iter()
            .any(|stop| matches!(stop, FocusStop::LeftPage));
        let kind = if left_page_before {
            EndingKind::Wrap
        } else {
            EndingKind::Trap
        };
        (kind, repeated + 1, 0)
    } else if matches!(walk.steps.last(), Some(FocusStop::LeftPage)) {
        let element_stops = walk
            .steps
            .iter()
            .filter(|stop| !matches!(stop, FocusStop::LeftPage))
            .count();
        (EndingKind::LeftPage, 0, element_stops)
    } else {
        (EndingKind::Ended, 0, steps)
    };
    Ending {
        kind,
        wording: wording(kind, step, count),
    }
}
