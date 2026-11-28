//! The checks of a snapshot's `sources` and `spans` that every adapter runs before it reads any
//! node: ids are unique, every span names a source, and every byte range is ordered and, when the
//! source's length is known, ends within it.

use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;
use std::hash::Hash;

use spokenpath_snapshot::{ByteRange, Snapshot};

use crate::error::excerpt;
use crate::{AdaptError, SpanRef};

/// The position of each source and each span by id, once both are checked.
pub(crate) struct SpanIndex {
    pub(crate) source_by_id: HashMap<u32, usize>,
    pub(crate) span_by_id: HashMap<u32, usize>,
}

impl SpanIndex {
    /// The location of the span with id `span_id`, from the span and its source; absent when the
    /// snapshot has no such span.
    pub(crate) fn span_ref(&self, snapshot: &Snapshot, span_id: u32) -> Option<SpanRef> {
        span_ref(snapshot, &self.span_by_id, &self.source_by_id, span_id)
    }
}

/// The location of the span with id `span_id`, from the span and the source it names; absent when
/// either is not in the snapshot.
pub(crate) fn span_ref(
    snapshot: &Snapshot,
    span_by_id: &HashMap<u32, usize>,
    source_by_id: &HashMap<u32, usize>,
    span_id: u32,
) -> Option<SpanRef> {
    let span = &snapshot.spans[*span_by_id.get(&span_id)?];
    let source = &snapshot.sources[*source_by_id.get(&span.source)?];
    Some(SpanRef {
        span: span.id,
        source: source.id,
        path: source.path.clone(),
        line: span.line,
        column: span.column,
        start_tag: crate::ByteRange {
            start: span.start_tag.start,
            end: span.start_tag.end,
        },
    })
}

/// Checks the snapshot's sources and spans, in the order the Chromium adapter always has: source
/// ids, span ids, then each span against its source.
pub(crate) fn check(
    snapshot: &Snapshot,
    source_lengths: &BTreeMap<u32, u64>,
) -> Result<SpanIndex, AdaptError> {
    let source_by_id = unique(snapshot.sources.iter().map(|source| source.id), |index| {
        format!("sources[{index}].id")
    })?;
    let span_by_id = unique(snapshot.spans.iter().map(|span| span.id), |index| {
        format!("spans[{index}].id")
    })?;
    check_spans(snapshot, &source_by_id, source_lengths)?;
    Ok(SpanIndex {
        source_by_id,
        span_by_id,
    })
}

/// Maps each key to its position, failing at the first key that repeats.
pub(crate) fn unique<K: Hash + Eq + Display>(
    keys: impl Iterator<Item = K>,
    path: impl Fn(usize) -> String,
) -> Result<HashMap<K, usize>, AdaptError> {
    let mut positions = HashMap::new();
    for (position, key) in keys.enumerate() {
        match positions.entry(key) {
            Entry::Occupied(first) => {
                return Err(AdaptError::field(
                    path(position),
                    format!(
                        "repeats {}, which index {} already holds",
                        excerpt(first.key()),
                        first.get()
                    ),
                ));
            }
            Entry::Vacant(slot) => {
                slot.insert(position);
            }
        }
    }
    Ok(positions)
}

fn check_spans(
    snapshot: &Snapshot,
    source_by_id: &HashMap<u32, usize>,
    source_lengths: &BTreeMap<u32, u64>,
) -> Result<(), AdaptError> {
    for (position, span) in snapshot.spans.iter().enumerate() {
        if !source_by_id.contains_key(&span.source) {
            return Err(AdaptError::field(
                format!("spans[{position}].source"),
                format!("names source {}, which is not in `sources`", span.source),
            ));
        }
        let length = source_lengths.get(&span.source).copied();
        check_range(
            &span.start_tag,
            length,
            span.source,
            &format!("spans[{position}].start_tag"),
        )?;
        if let Some(element) = &span.element {
            check_range(
                element,
                length,
                span.source,
                &format!("spans[{position}].element"),
            )?;
        }
    }
    Ok(())
}

fn check_range(
    range: &ByteRange,
    length: Option<u64>,
    source: u32,
    path: &str,
) -> Result<(), AdaptError> {
    if range.start > range.end {
        return Err(AdaptError::field(
            path,
            format!(
                "starts at byte {}, after its end {}",
                range.start, range.end
            ),
        ));
    }
    if let Some(length) = length
        && range.end > length
    {
        return Err(AdaptError::field(
            path,
            format!(
                "ends at byte {}, past the end of source {source}, which is {length} bytes long",
                range.end
            ),
        ));
    }
    Ok(())
}
