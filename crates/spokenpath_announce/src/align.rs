//! The alignment of a Chromium transcript with Safari's transcript of the same page, by source
//! location.
//!
//! Two items align when both are accessible items located by a span with the same span id; text
//! runs and items without a span never align, since nothing in Safari's tree says which Chromium
//! node they stand for. Every item of both transcripts is in exactly one row. Rows follow
//! Chromium's reading order: a Chromium item with a span starts, at the first Chromium item
//! carrying it, a row holding every item of both engines with that span, and any other Chromium
//! item is a row of its own. The Safari items in no aligned row (one that holds items of both
//! engines) are grouped the same way among themselves, and each group goes right after the rows
//! of the nearest earlier aligned Safari item in Safari's reading order and the Chromium-only rows
//! that follow them, in Safari's order; groups before the first aligned Safari item go right
//! before the first aligned row.

use std::collections::BTreeMap;

use spokenpath_model::{ExposedNode, Model, NodeIndex, SourceClass};

use crate::transcript::{Aligned, AlignedRow, Transcript};

/// Sets `chromium.aligned` to Safari's transcript and the rows that align the two. Each model is
/// the one its transcript was announced from; it gives each item's span.
pub fn align(
    chromium: &mut Transcript,
    chromium_model: &Model,
    safari: Transcript,
    safari_model: &Model,
) {
    let chromium_spans = item_spans(chromium_model, chromium.nodes.len());
    let safari_spans = item_spans(safari_model, safari.nodes.len());
    chromium.aligned = Some(Aligned {
        rows: rows(&chromium_spans, &safari_spans),
        safari: Box::new(safari),
    });
}

/// The span of each item, by item number less one: the span id of an accessible item located by
/// a span, and `None` for every other item. The walk is the one `announce` numbers items by.
fn item_spans(model: &Model, items: usize) -> Vec<Option<u32>> {
    let mut spans = Vec::with_capacity(items);
    let mut seen = vec![false; model.nodes.len()];
    let mut pending: Vec<NodeIndex> = vec![model.root];
    while let Some(index) = pending.pop() {
        let (Some(node), Some(slot)) = (model.node(index), seen.get_mut(index.get())) else {
            continue;
        };
        if std::mem::replace(slot, true) {
            continue;
        }
        spans.push(match node {
            ExposedNode::Accessible(node) => match &node.source {
                SourceClass::Span(span) => Some(span.span),
                _ => None,
            },
            ExposedNode::TextRun(_) => None,
        });
        pending.extend(node.children().iter().rev());
    }
    spans.resize(items, None);
    spans
}

/// The item numbers of each span, in reading order.
fn by_span(spans: &[Option<u32>]) -> BTreeMap<u32, Vec<usize>> {
    let mut numbers: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (index, span) in spans.iter().enumerate() {
        if let Some(span) = span {
            numbers.entry(*span).or_default().push(index + 1);
        }
    }
    numbers
}

/// The rows aligning items located by `chromium` spans with items located by `safari` spans,
/// each list holding one entry per item in reading order.
fn rows(chromium: &[Option<u32>], safari: &[Option<u32>]) -> Vec<AlignedRow> {
    let chromium_by_span = by_span(chromium);
    let safari_by_span = by_span(safari);

    // The rows in Chromium's order, each with whether it is aligned.
    let mut chromium_rows: Vec<(AlignedRow, bool)> = Vec::new();
    // The index in `chromium_rows` of each aligned Safari item's row, by item number less one.
    let mut aligned_row_of: Vec<Option<usize>> = vec![None; safari.len()];
    for (index, span) in chromium.iter().enumerate() {
        let number = index + 1;
        let row = match span {
            None => AlignedRow {
                chromium: vec![number],
                safari: Vec::new(),
            },
            Some(span) => {
                let chromium_items = &chromium_by_span[span];
                if chromium_items.first() != Some(&number) {
                    continue;
                }
                AlignedRow {
                    chromium: chromium_items.clone(),
                    safari: safari_by_span.get(span).cloned().unwrap_or_default(),
                }
            }
        };
        let aligned = !row.safari.is_empty();
        for &item in &row.safari {
            aligned_row_of[item - 1] = Some(chromium_rows.len());
        }
        chromium_rows.push((row, aligned));
    }

    // The Safari-only rows, in Safari's order, under the row of the nearest earlier aligned
    // Safari item, or under `None` before the first one.
    let mut anchored: BTreeMap<Option<usize>, Vec<AlignedRow>> = BTreeMap::new();
    let mut anchor = None;
    for (index, span) in safari.iter().enumerate() {
        let number = index + 1;
        if let Some(row) = aligned_row_of[index] {
            anchor = Some(row);
            continue;
        }
        let items = match span {
            Some(span) => {
                let items = &safari_by_span[span];
                if items.first() != Some(&number) {
                    continue;
                }
                items.clone()
            }
            None => vec![number],
        };
        anchored.entry(anchor).or_default().push(AlignedRow {
            chromium: Vec::new(),
            safari: items,
        });
    }

    let mut rows = Vec::with_capacity(chromium.len() + safari.len());
    let mut before_first = anchored.remove(&None);
    // The Safari-only rows of the last aligned row, placed once the Chromium-only rows after it
    // are.
    let mut after_last: Vec<AlignedRow> = Vec::new();
    for (position, (row, aligned)) in chromium_rows.into_iter().enumerate() {
        if aligned {
            rows.append(&mut after_last);
            if let Some(mut first) = before_first.take() {
                rows.append(&mut first);
            }
            after_last = anchored.remove(&Some(position)).unwrap_or_default();
        }
        rows.push(row);
    }
    rows.append(&mut after_last);
    // With no aligned row at all, every Safari item follows the Chromium rows.
    if let Some(mut rest) = before_first {
        rows.append(&mut rest);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::rows;
    use crate::transcript::AlignedRow;

    fn row(chromium: &[usize], safari: &[usize]) -> AlignedRow {
        AlignedRow {
            chromium: chromium.to_vec(),
            safari: safari.to_vec(),
        }
    }

    #[test]
    fn safari_only_items_follow_the_nearest_earlier_aligned_safari_item() {
        // Chromium: root, h1 (span 1), text, p (2), svg (3). Safari: root, h1, text, p, text.
        let chromium = [None, Some(1), None, Some(2), Some(3)];
        let safari = [None, Some(1), None, Some(2), None];
        assert_eq!(
            rows(&chromium, &safari),
            [
                row(&[1], &[]),
                row(&[], &[1]),
                row(&[2], &[2]),
                row(&[3], &[]),
                row(&[], &[3]),
                row(&[4], &[4]),
                row(&[5], &[]),
                row(&[], &[5]),
            ]
        );
    }

    #[test]
    fn a_span_carried_twice_is_one_row_at_its_first_chromium_item() {
        let chromium = [Some(1), Some(2), Some(1)];
        let safari = [Some(2), Some(1), Some(1), Some(9), Some(9)];
        assert_eq!(
            rows(&chromium, &safari),
            [row(&[1, 3], &[2, 3]), row(&[], &[4, 5]), row(&[2], &[1])]
        );
    }

    #[test]
    fn with_no_aligned_row_the_safari_items_follow_every_chromium_row() {
        assert_eq!(
            rows(&[None, Some(1)], &[None, Some(2)]),
            [
                row(&[1], &[]),
                row(&[2], &[]),
                row(&[], &[1]),
                row(&[], &[2])
            ]
        );
    }
}
