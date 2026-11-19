//! One run of the binary: read the snapshot and the source, adapt, announce, cross-check when
//! asked, write.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::{self, BufWriter, Read, Write};
use std::path::Path;

use sha2::{Digest, Sha256};
use spokenpath_announce::{Options, announce, cross_check, escape_for_terminal};
use spokenpath_model::chromium::adapt;
use spokenpath_model::safari_webdriver::{SafariAnswers, answers};
use spokenpath_snapshot::{Diagnostic, ReadError, Snapshot, read};

use crate::args::{self, Command, Format, Input, TranscriptArgs, USAGE};
use crate::text::{self, SourceText, TextContext};

/// The largest snapshot or source file the binary reads, in bytes. Adapting a snapshot costs more
/// than reading it, and not linearly in every shape, so the size is checked before parsing.
pub(crate) const MAX_INPUT_BYTES: u64 = 32 * 1024 * 1024;

/// [`MAX_INPUT_BYTES`] as the messages state it.
const MAX_INPUT_TEXT: &str = "32 MiB";

/// Why a run failed, each with its exit code.
#[derive(Debug)]
enum Failure {
    /// The command line is not one the binary accepts: exit 2, followed by the usage line.
    Usage(String),
    /// The snapshot or the source is unreadable, too large or invalid: exit 2.
    Input(String),
    /// The snapshot's major version is not one the reader reads: exit 3.
    UnsupportedVersion(String),
    /// Writing the output failed, for example to a closed pipe: exit 1.
    Output(String),
}

impl Failure {
    fn exit_code(&self) -> u8 {
        match self {
            Failure::Output(_) => 1,
            Failure::Usage(_) | Failure::Input(_) => 2,
            Failure::UnsupportedVersion(_) => 3,
        }
    }

    fn message(&self) -> &str {
        match self {
            Failure::Usage(message)
            | Failure::Input(message)
            | Failure::UnsupportedVersion(message)
            | Failure::Output(message) => message,
        }
    }
}

/// Runs the binary with `args` (without the program name) and returns its exit code. The tool
/// version is a parameter so tests can fix it.
pub(crate) fn run(
    args: impl IntoIterator<Item = OsString>,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    tool_version: &str,
) -> u8 {
    let result = match args::parse(args) {
        Err(message) => Err(Failure::Usage(message)),
        Ok(Command::Version) => {
            write_all(stdout, format!("{}\n", crate::version_line()).as_bytes())
        }
        Ok(Command::Transcript(args)) => transcript(&args, stdin, stdout, stderr, tool_version),
    };
    match result {
        Ok(()) => 0,
        Err(failure) => {
            let mut message = format!(
                "spokenpath-core: {}\n",
                escape_for_terminal(failure.message())
            );
            if matches!(failure, Failure::Usage(_)) {
                message.push_str(USAGE);
                message.push('\n');
            }
            // A failure to write the message to stderr has nowhere else to be reported; the exit
            // code still states the failure.
            let _ = stderr.write_all(message.as_bytes());
            failure.exit_code()
        }
    }
}

fn write_all(stdout: &mut dyn Write, bytes: &[u8]) -> Result<(), Failure> {
    stdout
        .write_all(bytes)
        .and_then(|()| stdout.flush())
        .map_err(|error| output_failure(&error))
}

fn output_failure(error: &io::Error) -> Failure {
    Failure::Output(format!("cannot write the transcript: {error}"))
}

fn transcript(
    args: &TranscriptArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    tool_version: &str,
) -> Result<(), Failure> {
    let snapshot = read_snapshot(&args.snapshot, stdin, "the snapshot")?;
    let source = args
        .source
        .as_deref()
        .map(|path| matched_source(&snapshot, path))
        .transpose()?;
    let lengths: BTreeMap<u32, u64> = source
        .iter()
        .flat_map(|source| source.ids.iter().map(|&id| (id, source.length)))
        .collect();
    let model = adapt(&snapshot, &lengths).map_err(|error| Failure::Input(error.to_string()))?;
    let safari = args
        .cross_check
        .as_ref()
        .map(|input| read_cross_check(input, stdin, &snapshot))
        .transpose()?;

    let mut diagnostics = String::new();
    for diagnostic in &model.diagnostics {
        // Writing to a String cannot fail.
        let _ = writeln!(
            diagnostics,
            "spokenpath-core: diagnostic {}: {}",
            escape_for_terminal(&diagnostic.code),
            escape_for_terminal(&diagnostic.message)
        );
    }
    if let Some((safari_snapshot, _)) = &safari {
        write_engine_diagnostics(&mut diagnostics, safari_snapshot);
    }
    // Diagnostics are advice: a stderr that cannot take them does not fail the transcript.
    let _ = stderr.write_all(diagnostics.as_bytes());

    let mut transcript = announce(
        &model,
        Options {
            layer_b: !args.no_speech,
        },
    );
    if let Some((_, answers)) = &safari {
        cross_check(&mut transcript, &model, answers);
    }
    let mut out = BufWriter::new(stdout);
    match args.format {
        Format::Json => {
            serde_json::to_writer_pretty(&mut out, &transcript).map_err(|error| {
                Failure::Output(format!("cannot write the transcript: {error}"))
            })?;
            out.write_all(b"\n")
                .map_err(|error| output_failure(&error))?;
        }
        Format::Text => {
            let context = TextContext {
                tool_version,
                snapshot_version: (snapshot.version.major, snapshot.version.minor),
                viewport: (
                    snapshot.capture.viewport.width,
                    snapshot.capture.viewport.height,
                ),
                layer_b: !args.no_speech,
                source: source.as_ref().map(|source| &source.text),
            };
            text::write_text(&mut out, &transcript, &context)
                .map_err(|error| output_failure(&error))?;
        }
    }
    out.flush().map_err(|error| output_failure(&error))
}

/// The bytes of `input`, at most [`MAX_INPUT_BYTES`]. A file's size is checked from its metadata
/// before it is opened, and every read is capped one byte past the limit, so neither a growing
/// file nor an endless stream is read in full.
fn read_limited(input: &Input, stdin: &mut dyn Read, what: &str) -> Result<Vec<u8>, Failure> {
    let (name, bytes) = match input {
        Input::Stdin => ("on stdin".to_owned(), read_capped(stdin)),
        Input::Path(path) => {
            let name = format!("`{}`", path.display());
            let too_large = std::fs::metadata(path)
                .map(|metadata| metadata.len() > MAX_INPUT_BYTES)
                .map_err(|error| Failure::Input(format!("cannot read {what} {name}: {error}")))?;
            if too_large {
                return Err(too_large_failure(what, &name));
            }
            let bytes = std::fs::File::open(path).and_then(|mut file| read_capped(&mut file));
            (name, bytes)
        }
    };
    let bytes =
        bytes.map_err(|error| Failure::Input(format!("cannot read {what} {name}: {error}")))?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(too_large_failure(what, &name));
    }
    Ok(bytes)
}

fn read_capped(reader: &mut dyn Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn too_large_failure(what: &str, name: &str) -> Failure {
    Failure::Input(format!(
        "{what} {name} is larger than the {MAX_INPUT_TEXT} input limit, so it is not parsed"
    ))
}

/// Reads the snapshot `what` names (`the snapshot` or `the cross-check snapshot`). The reader's
/// own message names the cross-check snapshot when it is that one, since both can fail alike.
fn read_snapshot(input: &Input, stdin: &mut dyn Read, what: &str) -> Result<Snapshot, Failure> {
    let bytes = read_limited(input, stdin, what)?;
    let text = String::from_utf8(bytes)
        .map_err(|error| Failure::Input(format!("{what} is not valid UTF-8: {error}")))?;
    let named = |error: &dyn std::fmt::Display| {
        if what == "the snapshot" {
            error.to_string()
        } else {
            format!("{what}: {error}")
        }
    };
    read(&text).map_err(|error| match error {
        ReadError::UnsupportedMajor { .. } => Failure::UnsupportedVersion(named(&error)),
        _ => Failure::Input(named(&error)),
    })
}

/// Reads the Safari snapshot to cross-check against, with Safari's answers, after checking that it
/// came from the same page as `snapshot`: the same sources and the same spans, so a span id names
/// the same start tag in both.
fn read_cross_check(
    input: &Input,
    stdin: &mut dyn Read,
    snapshot: &Snapshot,
) -> Result<(Snapshot, SafariAnswers), Failure> {
    let what = "the cross-check snapshot";
    let safari = read_snapshot(input, stdin, what)?;
    let answers = answers(&safari).map_err(|error| Failure::Input(format!("{what}: {error}")))?;
    if safari.sources != snapshot.sources || safari.spans != snapshot.spans {
        return Err(Failure::Input(
            "the cross-check snapshot was captured from a different page: its sources or spans \
             differ from the snapshot's"
                .to_owned(),
        ));
    }
    Ok((safari, answers))
}

/// Writes the diagnostics of a cross-check snapshot, each line naming its engine.
fn write_engine_diagnostics(out: &mut String, snapshot: &Snapshot) {
    let engine = snapshot.payload.engine();
    for Diagnostic { code, message, .. } in &snapshot.diagnostics {
        // Writing to a String cannot fail.
        let _ = writeln!(
            out,
            "spokenpath-core: diagnostic {engine} {}: {}",
            escape_for_terminal(code),
            escape_for_terminal(message)
        );
    }
}

/// The `--source` file and the snapshot sources whose SHA-256 it matches.
struct MatchedSource {
    ids: Vec<u32>,
    length: u64,
    text: SourceText,
}

/// Reads `path` and matches it to the snapshot's sources by SHA-256. The file's bytes stand for
/// every source with its hash. Excerpts are printed only for the paths of those sources, and
/// never for a path that another, unmatched source shares, since a span there may point into
/// different bytes.
fn matched_source(snapshot: &Snapshot, path: &Path) -> Result<MatchedSource, Failure> {
    let bytes = read_limited(
        &Input::Path(path.to_owned()),
        &mut io::empty(),
        "the source file",
    )?;
    let digest = Sha256::digest(&bytes);
    let mut hash = String::with_capacity(64);
    for byte in digest {
        // Writing to a String cannot fail.
        let _ = write!(hash, "{byte:02x}");
    }
    let (matched, unmatched): (Vec<_>, Vec<_>) = snapshot
        .sources
        .iter()
        .partition(|source| source.sha256 == hash);
    if matched.is_empty() {
        return Err(Failure::Input(format!(
            "the source file `{}` matches no source in the snapshot: its SHA-256 is {hash}; \
             it may have changed since it was captured, or be another file",
            path.display()
        )));
    }
    let shared: BTreeSet<&str> = unmatched
        .iter()
        .map(|source| source.path.as_str())
        .collect();
    let paths = matched
        .iter()
        .map(|source| source.path.as_str())
        .filter(|path| !shared.contains(path))
        .map(str::to_owned)
        .collect();
    Ok(MatchedSource {
        ids: matched.iter().map(|source| source.id).collect(),
        length: bytes.len() as u64,
        text: SourceText { paths, bytes },
    })
}
