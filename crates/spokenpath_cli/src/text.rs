//! The text transcript: a header, with a cross-check line when the transcript was cross-checked, a
//! legend, every item in reading order with its cross-check notes, the excluded elements, the
//! navigation indexes and the tab order. A part the engine does not supply says so in place of its
//! lines. An aligned transcript prints instead the two headers, a legend and one block per row,
//! each with the row's labels, its source location and each item's `A` line.
//!
//! Every page-controlled string passes through [`escape_for_terminal`] before it is written.
//! Layer B phrases and the tab-order ending are written as the announce crate renders them, since
//! the crate has already escaped them.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::{self, Write};

use spokenpath_announce::{
    AccessibleRecord, Aligned, AlignedRow, CrossCheck, DescriptionRecord, DescriptionSourceRecord,
    DomRecord, Excluded, ExclusionRecord, FieldCheck, Header, IndexEntry, Item, ItemCrossCheck,
    LayerA, Marking, NameRecord, NameSourceOutcome, NameSourceRecord, NameStatus, NotChecked,
    NotCheckedReason, RelatedRecord, RoleKind, RoleRecord, SourceRecord, StatesRecord, Stop,
    Transcript, TristateToken, escape_for_terminal,
};

/// The most characters of a start tag an excerpt shows before `…` marks the cut.
const EXCERPT_CHARS: usize = 80;

/// The deepest level the items are indented to. A deeper item is indented no further and states
/// its depth, so a deep tree's output grows linearly with its size, not with its depth squared.
const MAX_INDENT_DEPTH: usize = 32;

/// The source bytes the excerpts come from, and the source paths they stand for.
#[derive(Debug)]
pub(crate) struct SourceText {
    /// The snapshot source paths whose spans point into `bytes`.
    pub(crate) paths: BTreeSet<String>,
    /// The bytes of the file named by `--source`.
    pub(crate) bytes: Vec<u8>,
}

/// What the text needs beyond the transcript.
#[derive(Debug)]
pub(crate) struct TextContext<'a> {
    /// The spokenpath version the header names.
    pub(crate) tool_version: &'a str,
    /// The snapshot's major and minor version.
    pub(crate) snapshot_version: (u32, u32),
    /// The capture's viewport width and height.
    pub(crate) viewport: (u32, u32),
    /// Whether the items carry Layer B phrases, which sets the legend.
    pub(crate) layer_b: bool,
    /// The source bytes for excerpts; without them no excerpt is printed.
    pub(crate) source: Option<&'a SourceText>,
    /// The aligned snapshot's versions and viewport, when the transcript is aligned with one.
    pub(crate) aligned: Option<AlignedContext>,
}

/// What the aligned header line needs from the aligned snapshot.
#[derive(Debug)]
pub(crate) struct AlignedContext {
    /// The aligned snapshot's major and minor version.
    pub(crate) snapshot_version: (u32, u32),
    /// The aligned capture's viewport width and height.
    pub(crate) viewport: (u32, u32),
}

/// What a printer writes in place of a part the engine does not supply.
const NOT_AVAILABLE: &str = "not available from this engine";

fn esc(text: &str) -> Cow<'_, str> {
    escape_for_terminal(text)
}

/// Writes the text transcript of `transcript` to `out`.
pub(crate) fn write_text(
    out: &mut dyn Write,
    transcript: &Transcript,
    context: &TextContext<'_>,
) -> io::Result<()> {
    writeln!(
        out,
        "spokenpath {} · snapshot v{}.{} · {} · viewport {}x{}",
        esc(context.tool_version),
        context.snapshot_version.0,
        context.snapshot_version.1,
        engine_text(&transcript.header),
        context.viewport.0,
        context.viewport.1,
    )?;
    let printer = Printer {
        transcript,
        source: context.source,
        number_width: transcript.nodes.len().to_string().len(),
    };
    if let Some(aligned) = &transcript.aligned {
        return printer.aligned(out, aligned, context.aligned.as_ref());
    }
    if let Some(cross_check) = &transcript.cross_check {
        writeln!(out, "{}", cross_check_header(cross_check))?;
    }
    if context.layer_b {
        writeln!(
            out,
            "A = what the browser exposes   ~ = illustrative speech, approximate, not any specific screen reader"
        )?;
    } else {
        writeln!(out, "A = what the browser exposes")?;
    }

    writeln!(out)?;
    for item in &transcript.nodes {
        printer.item(out, item)?;
    }
    printer.excluded(out)?;
    printer.indexes(out)?;
    printer.tab_order(out)
}

struct Printer<'a> {
    transcript: &'a Transcript,
    source: Option<&'a SourceText>,
    number_width: usize,
}

impl Printer<'_> {
    fn item(&self, out: &mut dyn Write, item: &Item) -> io::Result<()> {
        let indent = "  ".repeat(item.depth.min(MAX_INDENT_DEPTH));
        let depth_note = if item.depth > MAX_INDENT_DEPTH {
            format!("[depth {}] ", item.depth)
        } else {
            String::new()
        };
        let width = self.number_width;
        let layer = format!("{:width$}  {indent}", "");
        let detail = format!("{layer}  ");
        writeln!(
            out,
            "{:>width$}  {indent}{depth_note}A {}",
            item.number,
            a_line(&item.layer_a)
        )?;
        if let LayerA::Accessible(record) = &item.layer_a {
            writeln!(out, "{detail}{}", self.location(&record.source))?;
            for line in name_lines(&record.name) {
                writeln!(out, "{detail}{line}")?;
            }
            if let Some(description) = &record.description {
                writeln!(out, "{detail}{}", description_line(description))?;
            }
        }
        if let (Some(check), Some(summary)) = (&item.cross_check, &self.transcript.cross_check) {
            let chromium = &self.transcript.header.engine_version;
            for line in cross_check_notes(check, &summary.engine_version, chromium) {
                writeln!(out, "{detail}{line}")?;
            }
        }
        if let Some(approx) = &item.approx {
            writeln!(out, "{layer}~ \"{}\"", approx.phrase)?;
        }
        Ok(())
    }

    /// The aligned layout after the first header line: the aligned snapshot's header, the legend,
    /// and one block per row.
    fn aligned(
        &self,
        out: &mut dyn Write,
        aligned: &Aligned,
        context: Option<&AlignedContext>,
    ) -> io::Result<()> {
        let safari = &aligned.safari;
        let (version, viewport) = context.map_or(((0, 0), (0, 0)), |context| {
            (context.snapshot_version, context.viewport)
        });
        writeln!(
            out,
            "aligned with · snapshot v{}.{} · {} · viewport {}x{}",
            version.0,
            version.1,
            engine_text(&safari.header),
            viewport.0,
            viewport.1,
        )?;
        writeln!(
            out,
            "A = what the browser exposes   C<n> = Chromium item n, S<n> = Safari item n, aligned by source location"
        )?;
        writeln!(out)?;
        // A label is its letter and the item number, so each column is as wide as its largest.
        let label_width = |items: &[Item]| 1 + items.len().to_string().len();
        let chromium_width = label_width(&self.transcript.nodes);
        let safari_width = label_width(&safari.nodes);
        let indent = " ".repeat(chromium_width + 2 + safari_width + 2);
        for row in &aligned.rows {
            self.aligned_row(out, row, safari, (chromium_width, safari_width), &indent)?;
        }
        Ok(())
    }

    fn aligned_row(
        &self,
        out: &mut dyn Write,
        row: &AlignedRow,
        safari: &Transcript,
        (chromium_width, safari_width): (usize, usize),
        indent: &str,
    ) -> io::Result<()> {
        let labels = |prefix: char, numbers: &[usize]| {
            numbers
                .iter()
                .map(|number| format!("{prefix}{number}"))
                .collect::<Vec<_>>()
        };
        let chromium_labels = labels('C', &row.chromium);
        let safari_labels = labels('S', &row.safari);
        let chromium_items = items_numbered(&self.transcript.nodes, &row.chromium);
        let safari_items = items_numbered(&safari.nodes, &row.safari);
        let first = chromium_items.first().or(safari_items.first());
        let location = match first.map(|item| &item.layer_a) {
            Some(LayerA::Accessible(record)) => self.location(&record.source),
            Some(LayerA::TextRun(_)) => {
                format!("no source location: {}", no_location_reason("text"))
            }
            None => String::new(),
        };
        // One label line per pair of labels; a row of one span carried twice lists every number.
        let lines = chromium_labels.len().max(safari_labels.len()).max(1);
        for line in 0..lines {
            let chromium = chromium_labels.get(line).map_or("", String::as_str);
            let safari = safari_labels.get(line).map_or("", String::as_str);
            let start = format!("{chromium:<chromium_width$}  {safari:<safari_width$}  ");
            if line == 0 {
                writeln!(out, "{start}{location}")?;
            } else {
                writeln!(out, "{}", start.trim_end())?;
            }
        }
        for (engine, items) in [("Chromium", &chromium_items), ("Safari  ", &safari_items)] {
            for item in items {
                writeln!(out, "{indent}A {engine}  {}", a_line(&item.layer_a))?;
            }
        }
        let spanned = first.is_some_and(|item| {
            matches!(&item.layer_a, LayerA::Accessible(record) if record.source.class == "span")
        });
        if spanned && safari_items.is_empty() {
            writeln!(
                out,
                "{indent}  only in Chromium: no Safari item has this source location"
            )?;
        } else if spanned && chromium_items.is_empty() {
            writeln!(
                out,
                "{indent}  only in Safari: no Chromium item has this source location"
            )?;
        }
        Ok(())
    }

    /// The source location of a node: `path:line:column` and the excerpt for a span, otherwise
    /// the statement that there is none and why.
    fn location(&self, source: &SourceRecord) -> String {
        let Some(location) = span_location(source) else {
            return format!("no source location: {}", no_location_reason(source.class));
        };
        match self.excerpt(source) {
            Some(excerpt) => format!("{location}  {excerpt}"),
            None => location,
        }
    }

    /// The span's start tag from the source bytes, decoded lossily, each whitespace run collapsed
    /// to one space and cut to [`EXCERPT_CHARS`] characters.
    fn excerpt(&self, source: &SourceRecord) -> Option<String> {
        let text = self.source?;
        if !text.paths.contains(source.path.as_deref()?) {
            return None;
        }
        let range = source.start_tag?;
        let start = usize::try_from(range.start).ok()?;
        let end = usize::try_from(range.end).ok()?;
        let bytes = text.bytes.get(start..end)?;
        let decoded = String::from_utf8_lossy(bytes);
        let collapsed = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
        let cut = match collapsed.char_indices().nth(EXCERPT_CHARS) {
            Some((at, _)) => format!("{}…", &collapsed[..at]),
            None => collapsed,
        };
        Some(esc(&cut).into_owned())
    }

    fn excluded(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "\nExcluded")?;
        let Some(excluded) = &self.transcript.excluded else {
            return writeln!(out, "  {NOT_AVAILABLE}");
        };
        if excluded.is_empty() {
            writeln!(out, "  (none)")?;
        }
        for excluded in excluded {
            self.excluded_entry(out, excluded)?;
        }
        Ok(())
    }

    fn excluded_entry(&self, out: &mut dyn Write, excluded: &Excluded) -> io::Result<()> {
        writeln!(
            out,
            "  {}  {}",
            element(&excluded.dom),
            self.location(&excluded.source)
        )?;
        if excluded.reasons.is_empty() {
            writeln!(out, "    no reason given by the engine")?;
        }
        for reason in &excluded.reasons {
            writeln!(out, "    {}", exclusion(reason))?;
        }
        Ok(())
    }

    fn indexes(&self, out: &mut dyn Write) -> io::Result<()> {
        let indexes = &self.transcript.indexes;
        writeln!(out, "\nHeadings")?;
        if indexes.headings.is_empty() {
            writeln!(out, "  (none)")?;
        }
        for heading in &indexes.headings {
            let level = heading
                .level
                .map_or_else(String::new, |level| format!("  level {level}"));
            writeln!(out, "  {}{level}", index_entry(&heading.entry))?;
        }
        for (title, entries) in [
            ("Landmarks", &indexes.landmarks),
            ("Links", &indexes.links),
            ("Form controls", &indexes.form_controls),
        ] {
            writeln!(out, "\n{title}")?;
            if entries.is_empty() {
                writeln!(out, "  (none)")?;
            }
            for entry in entries {
                writeln!(out, "  {}", index_entry(entry))?;
            }
        }
        Ok(())
    }

    fn tab_order(&self, out: &mut dyn Write) -> io::Result<()> {
        writeln!(out, "\nTab order")?;
        let Some(tab_order) = &self.transcript.tab_order else {
            return writeln!(out, "  {NOT_AVAILABLE}");
        };
        if tab_order.steps.is_empty() {
            writeln!(out, "  (none)")?;
        }
        let width = tab_order.steps.len().to_string().len();
        for step in &tab_order.steps {
            let repeats = step
                .repeats
                .map_or_else(String::new, |repeated| format!("  repeats step {repeated}"));
            writeln!(
                out,
                "  {:>width$}  {}{repeats}",
                step.step,
                self.stop(&step.stop)
            )?;
        }
        writeln!(
            out,
            "  ending ({}): {}",
            marking(tab_order.ending.by),
            tab_order.ending.value.wording
        )
    }

    fn stop(&self, stop: &Stop) -> String {
        match stop {
            Stop::LeftPage => "left the page".to_owned(),
            Stop::Unresolved { backend_id } => {
                format!("an element the snapshot's DOM does not hold (engine node {backend_id})")
            }
            Stop::Element {
                item: Some(number), ..
            } => self.cited(*number),
            Stop::Element { dom, source, .. } => {
                let location = source.as_ref().map_or_else(
                    || "no source location held".to_owned(),
                    |source| self.location(source),
                );
                format!("{}  {location}  (not an item)", element(dom))
            }
        }
    }

    /// An item cited by number with its role and name, as an index entry cites it.
    fn cited(&self, number: usize) -> String {
        let record = number
            .checked_sub(1)
            .and_then(|index| self.transcript.nodes.get(index))
            .map(|item| &item.layer_a);
        match record {
            Some(LayerA::Accessible(record)) => {
                format!(
                    "[{number}] {} {}",
                    role(record.role.as_ref()),
                    name(&record.name)
                )
            }
            Some(LayerA::TextRun(run)) => format!("[{number}] text \"{}\"", esc(&run.text)),
            None => format!("[{number}]"),
        }
    }
}

/// The items with the given numbers, in that order; a number with no item is skipped.
fn items_numbered<'a>(nodes: &'a [Item], numbers: &[usize]) -> Vec<&'a Item> {
    numbers
        .iter()
        .filter_map(|&number| number.checked_sub(1).and_then(|index| nodes.get(index)))
        .collect()
}

/// The `A` line after its marker: role, name, value, states and set position, or a text run's
/// text.
fn a_line(layer_a: &LayerA) -> String {
    let record: &AccessibleRecord = match layer_a {
        LayerA::TextRun(run) => return format!("text \"{}\"", esc(&run.text)),
        LayerA::Accessible(record) => record,
    };
    let mut parts = vec![format!(
        "{} {}",
        role(record.role.as_ref()),
        name(&record.name)
    )];
    if let Some(value) = &record.value {
        parts.push(format!("value \"{}\"", esc(value)));
    }
    let states = state_words(&record.states);
    if !states.is_empty() {
        parts.push(states.join(", "));
    }
    if let Some(position) = &record.set_position {
        let value = position.value;
        let counted = match value.size {
            Some(size) => format!("{} of {size}", value.position),
            None => format!("position {}", value.position),
        };
        parts.push(format!("{counted} ({})", marking(position.by)));
    }
    parts.join("  ")
}

fn role(role: Option<&RoleRecord>) -> String {
    match role {
        None => "(no role)".to_owned(),
        Some(role) => match role.kind {
            RoleKind::Aria => esc(&role.token).into_owned(),
            RoleKind::Internal => format!("{} (internal role)", esc(&role.token)),
            RoleKind::Platform => match &role.subrole {
                Some(subrole) => {
                    format!("{}/{} (platform role)", esc(&role.token), esc(subrole))
                }
                None => format!("{} (platform role)", esc(&role.token)),
            },
        },
    }
}

fn name(name: &NameRecord) -> String {
    match name.named_text() {
        Some(text) => format!("\"{}\"", esc(text)),
        None => "(no accessible name)".to_owned(),
    }
}

/// The exposed states as words. A boolean state shows only when true, except `expanded`, which
/// shows as `expanded` or `collapsed`; an `invalid` token of `false` is left out.
fn state_words(states: &StatesRecord) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    if let Some(checked) = states.checked {
        words.push(
            match checked {
                TristateToken::True => "checked",
                TristateToken::False => "not checked",
                TristateToken::Mixed => "checked mixed",
            }
            .to_owned(),
        );
    }
    if let Some(pressed) = states.pressed {
        words.push(
            match pressed {
                TristateToken::True => "pressed",
                TristateToken::False => "not pressed",
                TristateToken::Mixed => "pressed mixed",
            }
            .to_owned(),
        );
    }
    if let Some(expanded) = states.expanded {
        words.push(if expanded { "expanded" } else { "collapsed" }.to_owned());
    }
    for (state, word) in [
        (states.selected, "selected"),
        (states.disabled, "disabled"),
        (states.required, "required"),
        (states.readonly, "read-only"),
    ] {
        if state == Some(true) {
            words.push(word.to_owned());
        }
    }
    match states.invalid.as_deref() {
        None | Some("false") => {}
        Some("true") => words.push("invalid".to_owned()),
        Some(token) => words.push(format!("invalid ({})", esc(token))),
    }
    for (state, word) in [(states.focusable, "focusable"), (states.focused, "focused")] {
        if state == Some(true) {
            words.push(word.to_owned());
        }
    }
    if let Some(level) = states.level {
        words.push(format!("level {level}"));
    }
    words
}

fn marking(by: Marking) -> &'static str {
    match by {
        Marking::ExposedByEngine => "exposed by engine",
        Marking::ComputedByTool => "computed by tool",
    }
}

fn span_location(source: &SourceRecord) -> Option<String> {
    if source.class != "span" {
        return None;
    }
    Some(format!(
        "{}:{}:{}",
        esc(source.path.as_deref()?),
        source.line?,
        source.column?
    ))
}

fn no_location_reason(class: &str) -> String {
    match class {
        "implied" => "the HTML parser inserted this element (implied)".to_owned(),
        "script" => "a page script created this element (script)".to_owned(),
        "library" => "a known library created this element (library)".to_owned(),
        "unknown" => "its origin could not be established (unknown)".to_owned(),
        "document" => "the document itself (document)".to_owned(),
        "not-in-dom" => "the snapshot's DOM does not hold this node (not-in-dom)".to_owned(),
        "text" => "a text node (text)".to_owned(),
        other => format!("({})", esc(other)),
    }
}

/// A related node's short location: `path:line:column`, or why it has none.
fn short_location(source: Option<&SourceRecord>) -> String {
    match source {
        None => "no source location held".to_owned(),
        Some(source) => span_location(source)
            .unwrap_or_else(|| format!("no source location ({})", esc(source.class))),
    }
}

fn element(dom: &DomRecord) -> String {
    let mut text = match &dom.tag {
        Some(tag) => format!("<{}>", esc(tag)),
        None => format!("node {}", dom.dom_id),
    };
    if let Some(id) = &dom.id {
        // Writing to a String cannot fail.
        let _ = write!(text, " #{}", esc(id));
    }
    text
}

fn related(node: &RelatedRecord) -> String {
    let mut parts = Vec::new();
    match (&node.dom, &node.idref) {
        (Some(dom), _) => parts.push(element(dom)),
        (None, Some(idref)) => parts.push(format!("idref \"{}\"", esc(idref))),
        (None, None) => parts.push("a node".to_owned()),
    }
    if let Some(text) = &node.text {
        parts.push(format!("\"{}\"", esc(text)));
    }
    parts.push(short_location(node.source.as_ref()));
    parts.join("  ")
}

/// One name source as `what (kind)`: the attribute, with its value when the engine gives one,
/// or the native mechanism, or the kind alone.
fn describe_source(source: &NameSourceRecord) -> String {
    let kind = esc(&source.kind);
    match (&source.attribute, &source.native_source) {
        (Some(attribute), _) => match &source.attribute_value {
            Some(value) => format!("{}=\"{}\" ({kind})", esc(attribute), esc(value)),
            None => format!("{} ({kind})", esc(attribute)),
        },
        (None, Some(native)) => format!("{} ({kind})", esc(native)),
        (None, None) => kind.into_owned(),
    }
}

fn outcome(source: &NameSourceRecord) -> String {
    match source.outcome {
        NameSourceOutcome::Used => "used".to_owned(),
        NameSourceOutcome::Superseded => "superseded".to_owned(),
        NameSourceOutcome::Empty => "empty".to_owned(),
        NameSourceOutcome::Invalid => match &source.invalid_reason {
            Some(reason) => format!("invalid ({})", esc(reason)),
            None => "invalid".to_owned(),
        },
    }
}

fn source_list<'a>(sources: impl Iterator<Item = &'a NameSourceRecord>) -> String {
    sources
        .map(|source| format!("{}: {}", describe_source(source), outcome(source)))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The name provenance: the winning source with its related nodes and the other sources for a
/// named item, or every source tried for an unnamed one.
fn name_lines(name: &NameRecord) -> Vec<String> {
    let mut lines = Vec::new();
    let Some(sources) = &name.sources else {
        lines.push(match name.status {
            NameStatus::Named => format!("name sources {NOT_AVAILABLE}"),
            NameStatus::NoAccessibleName => {
                format!("no accessible name; name sources {NOT_AVAILABLE}")
            }
        });
        return lines;
    };
    if name.status == NameStatus::NoAccessibleName {
        if sources.is_empty() {
            lines.push("no accessible name; the engine reported no name sources".to_owned());
        } else {
            lines.push(format!(
                "no accessible name; tried: {}",
                source_list(sources.iter())
            ));
        }
        return lines;
    }
    let winner = sources
        .iter()
        .position(|source| source.outcome == NameSourceOutcome::Used);
    let hidden = match &name.from_hidden_content {
        Some(marked) if marked.value => format!(", from hidden content ({})", marking(marked.by)),
        _ => String::new(),
    };
    match winner.map(|index| &sources[index]) {
        Some(source) => {
            lines.push(format!("name from {}{hidden}", describe_source(source)));
            for node in &source.related_nodes {
                lines.push(format!("  via {}", related(node)));
            }
        }
        None => lines.push(format!(
            "name from a source the engine does not report{hidden}"
        )),
    }
    let others: Vec<&NameSourceRecord> = sources
        .iter()
        .enumerate()
        .filter(|&(index, _)| Some(index) != winner)
        .map(|(_, source)| source)
        .collect();
    if !others.is_empty() {
        lines.push(format!(
            "other sources: {}",
            source_list(others.into_iter())
        ));
    }
    lines
}

fn description_line(description: &DescriptionRecord) -> String {
    let text = esc(&description.text);
    let Some(provenance) = &description.provenance else {
        return format!("description \"{text}\"  (its source is not attributed)");
    };
    let from = match &provenance.value {
        DescriptionSourceRecord::AriaDescribedby { elements } => {
            let elements: Vec<String> = elements.iter().map(related).collect();
            if elements.is_empty() {
                "aria-describedby".to_owned()
            } else {
                format!("aria-describedby: {}", elements.join("; "))
            }
        }
        DescriptionSourceRecord::AriaDescription => "aria-description".to_owned(),
        DescriptionSourceRecord::Title => "title".to_owned(),
    };
    format!(
        "description \"{text}\"  from {from} ({})",
        marking(provenance.by)
    )
}

fn exclusion(reason: &ExclusionRecord) -> String {
    let mut line = format!(
        "{} (engine reason: {})",
        esc(reason.kind),
        esc(&reason.engine_reason)
    );
    if let Some(ancestor) = &reason.hiding_ancestor {
        // Writing to a String cannot fail.
        let _ = write!(
            line,
            "; hidden by {}  {} ({})",
            element(&ancestor.value.dom),
            short_location(ancestor.value.source.as_ref()),
            marking(ancestor.by)
        );
    }
    line
}

fn index_entry(entry: &IndexEntry) -> String {
    let name = match (entry.name_status, &entry.name) {
        (NameStatus::Named, Some(text)) => format!("\"{}\"", esc(text)),
        _ => "(no accessible name)".to_owned(),
    };
    format!("[{}] {} {name}", entry.item, esc(&entry.role))
}

/// `Safari <version> on macOS <version> (<build>)`, without the macOS part, or its build, when
/// the capture did not record it.
fn safari_platform(version: &str, macos: Option<&str>, build: Option<&str>) -> String {
    let mut platform = format!("Safari {}", esc(version));
    if let Some(macos) = macos {
        // Writing to a String cannot fail.
        let _ = write!(platform, " on macOS {}", esc(macos));
        if let Some(build) = build {
            let _ = write!(platform, " ({})", esc(build));
        }
    }
    platform
}

/// The engine and its version as the header names them.
fn engine_text(header: &Header) -> String {
    match header.engine {
        "chromium" => format!("Chromium {}", esc(&header.engine_version)),
        "safari-ax" => safari_platform(
            &header.engine_version,
            header.macos_version.as_deref(),
            header.macos_build.as_deref(),
        ),
        other => format!("{} {}", esc(other), esc(&header.engine_version)),
    }
}

/// The second header line of a cross-checked transcript: the Safari and macOS versions, and what
/// the comparison found.
fn cross_check_header(cross_check: &CrossCheck) -> String {
    let platform = safari_platform(
        &cross_check.engine_version,
        cross_check.macos_version.as_deref(),
        cross_check.macos_build.as_deref(),
    );
    let elements = if cross_check.compared == 1 {
        "element"
    } else {
        "elements"
    };
    let differ = if cross_check.differs == 1 {
        "differs"
    } else {
        "differ"
    };
    format!(
        "cross-check · {platform} over WebDriver · role and name of {} {elements} compared: {} {differ}, {} not checked",
        cross_check.compared, cross_check.differs, cross_check.not_checked
    )
}

/// The note lines of one compared item: one per field that differs or is not checked, and one for
/// both fields when neither is checked for the same reason. A field that is the same in both
/// engines gets no line. No line says which engine is right.
fn cross_check_notes(check: &ItemCrossCheck, safari: &str, chromium: &str) -> Vec<String> {
    let (safari, chromium) = (esc(safari), esc(chromium));
    if let (FieldCheck::NotChecked { error: role }, FieldCheck::NotChecked { error: name }) =
        (&check.role, &check.name)
        && role == name
    {
        return vec![format!(
            "not checked in Safari {safari}: role and name ({})",
            not_checked_reason(role)
        )];
    }
    check
        .fields()
        .into_iter()
        .filter_map(|(field, outcome)| match outcome {
            FieldCheck::Same { .. } => None,
            FieldCheck::Differs {
                safari: safari_value,
                chromium: chromium_value,
            } => Some(format!(
                "differs in Safari: {field} \"{}\" in Safari {safari}, \"{}\" in Chromium {chromium}",
                esc(safari_value),
                esc(chromium_value)
            )),
            FieldCheck::NotChecked { error } => Some(format!(
                "not checked in Safari {safari}: {field} ({})",
                not_checked_reason(error)
            )),
        })
        .collect()
}

fn not_checked_reason(reason: &NotChecked) -> String {
    match reason {
        NotChecked::Engine(error) if error.message.is_empty() => {
            format!("Safari answered with error \"{}\"", esc(&error.error))
        }
        NotChecked::Engine(error) => format!(
            "Safari answered with error \"{}\": {}",
            esc(&error.error),
            esc(&error.message)
        ),
        NotChecked::Reason(NotCheckedReason::NoSafariElement) => {
            "no element with this tag was found in Safari".to_owned()
        }
        NotChecked::Reason(NotCheckedReason::SpanNotUnique) => {
            "more than one element has this tag".to_owned()
        }
    }
}
