use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde_json::{Value, json};
use spokenpath_announce::{Options, announce};
use spokenpath_model::chromium::adapt;
use spokenpath_snapshot::read;

use crate::run::{MAX_INPUT_BYTES, run};
use crate::version_line;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn goldens_directory() -> PathBuf {
    fixtures().join("snapshots/chromium")
}

/// Every capture under `fixtures/snapshots/chromium/`, relative to it, found by listing the
/// directories, so a new capture without a text golden fails the golden test.
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

fn golden_path(name: &str) -> PathBuf {
    goldens_directory().join(name)
}

fn golden_value(name: &Path) -> Value {
    let path = goldens_directory().join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is JSON: {error}", path.display()))
}

/// The page of a capture's first source, under `fixtures/pages/` at the capture's relative
/// directory. The path is canonicalised and must stay under `fixtures/pages/` before it is used,
/// because the source path comes from the snapshot.
fn page_of(name: &Path) -> PathBuf {
    let pages = fixtures()
        .join("pages")
        .canonicalize()
        .expect("fixtures/pages exists");
    let value = golden_value(name);
    let source = value["sources"][0]["path"]
        .as_str()
        .unwrap_or_else(|| panic!("{} has a source path", name.display()));
    let page = pages
        .join(name.parent().unwrap_or(Path::new("")))
        .join(source)
        .canonicalize()
        .unwrap_or_else(|error| panic!("{} has its page: {error}", name.display()));
    assert!(
        page.starts_with(&pages),
        "{} names a source outside fixtures/pages: {}",
        name.display(),
        page.display()
    );
    page
}

/// The insta snapshot name of a capture: its relative path without the extension, with `__`
/// between directories, the same on every platform.
fn snapshot_name(name: &Path) -> String {
    name.with_extension("")
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("__")
}

#[derive(Debug)]
struct Run {
    code: u8,
    stdout: String,
    stderr: String,
}

fn run_with(args: &[&OsString], stdin: &mut dyn Read) -> Run {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run(
        args.iter().map(|&arg| arg.clone()),
        stdin,
        &mut stdout,
        &mut stderr,
        "0.0.0",
    );
    Run {
        code,
        stdout: String::from_utf8(stdout).expect("stdout is UTF-8"),
        stderr: String::from_utf8(stderr).expect("stderr is UTF-8"),
    }
}

/// Runs the binary with `args` and an empty stdin.
fn run_args(args: &[&str]) -> Run {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    run_with(&args.iter().collect::<Vec<_>>(), &mut io::empty())
}

fn run_paths(args: &[&dyn AsRef<std::ffi::OsStr>]) -> Run {
    let args: Vec<OsString> = args.iter().map(|arg| arg.as_ref().to_owned()).collect();
    run_with(&args.iter().collect::<Vec<_>>(), &mut io::empty())
}

/// Runs `transcript -` with `stdin` and the extra `args`.
fn run_stdin(stdin: &[u8], args: &[&str]) -> Run {
    let mut all: Vec<OsString> = vec!["transcript".into(), "-".into()];
    all.extend(args.iter().map(OsString::from));
    run_with(&all.iter().collect::<Vec<_>>(), &mut &stdin[..])
}

/// A fresh directory under the system temporary directory, unique to this process and test,
/// removed when the value drops.
struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn new(name: &str) -> Self {
        let directory =
            std::env::temp_dir().join(format!("spokenpath-core-{}-{name}", std::process::id()));
        if directory.exists() {
            std::fs::remove_dir_all(&directory).expect("stale test directory is removed");
        }
        std::fs::create_dir_all(&directory).expect("test directory is created");
        Self(directory)
    }

    fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.0.join(path)
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            eprintln!("could not remove {}: {error}", self.0.display());
        }
    }
}

fn sign_in_bytes() -> Vec<u8> {
    std::fs::read(golden_path("sign-in.json")).expect("sign-in.json is readable")
}

fn assert_one_failure_line(run: &Run, code: u8) {
    assert_eq!(run.code, code, "{run:?}");
    assert_eq!(run.stdout, "", "{run:?}");
    let first = run.stderr.lines().next().unwrap_or_default();
    assert!(first.starts_with("spokenpath-core: "), "{run:?}");
}

#[test]
fn version_line_names_the_binary_and_its_version() {
    assert_eq!(
        version_line(),
        concat!("spokenpath-core ", env!("CARGO_PKG_VERSION"))
    );
    let run = run_args(&["--version"]);
    assert_eq!(run.code, 0, "{run:?}");
    assert_eq!(run.stdout, format!("{}\n", version_line()));
    assert_eq!(run.stderr, "");
}

#[test]
fn every_capture_matches_its_text_golden() {
    let names = golden_names();
    assert!(names.iter().any(|name| name == Path::new("sign-in.json")));
    for name in &names {
        let run = run_paths(&[
            &"transcript",
            &goldens_directory().join(name),
            &"--source",
            &page_of(name),
        ]);
        assert_eq!(run.code, 0, "{}: {run:?}", name.display());
        insta::assert_snapshot!(snapshot_name(name), run.stdout);
    }
}

#[test]
fn sign_in_without_speech_matches_its_golden_and_has_no_layer_b() {
    let name = Path::new("sign-in.json");
    let run = run_paths(&[
        &"transcript",
        &goldens_directory().join(name),
        &"--source",
        &page_of(name),
        &"--no-speech",
    ]);
    assert_eq!(run.code, 0, "{run:?}");
    assert!(!run.stdout.contains('~'), "{}", run.stdout);
    assert!(
        !run.stdout.contains("illustrative speech"),
        "{}",
        run.stdout
    );
    insta::assert_snapshot!("sign-in__no-speech", run.stdout);
}

#[test]
fn malformed_json_exits_2_naming_the_problem() {
    let run = run_stdin(b"{", &[]);
    assert_one_failure_line(&run, 2);
    assert!(run.stderr.contains("not valid JSON"), "{run:?}");
}

#[test]
fn a_non_snapshot_exits_2() {
    let run = run_stdin(b"{}", &[]);
    assert_one_failure_line(&run, 2);
    assert!(run.stderr.contains("not a spokenpath snapshot"), "{run:?}");
}

#[test]
fn a_dangling_reference_exits_2_naming_its_dotted_path() {
    let mut value = golden_value(Path::new("sign-in.json"));
    value["spans"][2]["source"] = json!(7);
    let run = run_stdin(value.to_string().as_bytes(), &[]);
    assert_one_failure_line(&run, 2);
    assert!(run.stderr.contains("`spans[2].source`"), "{run:?}");
}

#[test]
fn an_unsupported_major_version_exits_3_naming_both_majors() {
    let mut value = golden_value(Path::new("sign-in.json"));
    value["version"]["major"] = json!(1);
    let run = run_stdin(value.to_string().as_bytes(), &[]);
    assert_one_failure_line(&run, 3);
    assert!(run.stderr.contains("major version 1"), "{run:?}");
    assert!(run.stderr.contains("major version 0"), "{run:?}");
}

#[test]
fn usage_errors_exit_2_with_the_usage_line() {
    let snapshot = golden_path("sign-in.json");
    let snapshot = snapshot.to_str().expect("a UTF-8 path");
    for (case, args, says) in [
        ("no command", vec![], "no command"),
        ("an unknown command", vec!["render"], "`render`"),
        (
            "an unknown flag",
            vec!["transcript", snapshot, "--colour"],
            "`--colour`",
        ),
        ("no snapshot", vec!["transcript"], "no snapshot"),
        (
            "two snapshots",
            vec!["transcript", snapshot, "-"],
            "more than one",
        ),
        (
            "a missing value",
            vec!["transcript", snapshot, "--format"],
            "needs a value",
        ),
        (
            "an unknown format",
            vec!["transcript", snapshot, "--format", "xml"],
            "`xml`",
        ),
        (
            "a repeated format",
            vec![
                "transcript",
                snapshot,
                "--format",
                "json",
                "--format",
                "json",
            ],
            "more than once",
        ),
        (
            "a repeated --no-speech",
            vec!["transcript", snapshot, "--no-speech", "--no-speech"],
            "more than once",
        ),
        (
            "--version with an argument",
            vec!["--version", "x"],
            "no arguments",
        ),
    ] {
        let run = run_args(&args);
        assert_one_failure_line(&run, 2);
        assert!(run.stderr.contains(says), "{case}: {run:?}");
        let lines: Vec<&str> = run.stderr.lines().collect();
        assert_eq!(lines.len(), 2, "{case}: {run:?}");
        assert!(
            lines[1].starts_with("usage: spokenpath-core"),
            "{case}: {run:?}"
        );
    }
}

#[test]
fn an_unreadable_snapshot_exits_2() {
    let directory = TemporaryDirectory::new("unreadable");
    let run = run_paths(&[&"transcript", &directory.join("missing.json")]);
    assert_one_failure_line(&run, 2);
    assert!(run.stderr.contains("cannot read the snapshot"), "{run:?}");
}

/// A reader that never ends.
struct Endless;

impl Read for Endless {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        buffer.fill(b' ');
        Ok(buffer.len())
    }
}

#[test]
fn an_oversized_input_exits_2_naming_the_limit_without_parsing() {
    let args: Vec<OsString> = vec!["transcript".into(), "-".into()];
    let run = run_with(&args.iter().collect::<Vec<_>>(), &mut Endless);
    assert_one_failure_line(&run, 2);
    assert!(run.stderr.contains("32 MiB input limit"), "{run:?}");

    // Whitespace is valid JSON padding, so only the size check can reject this file.
    let directory = TemporaryDirectory::new("oversized");
    let path = directory.join("large.json");
    let mut bytes = sign_in_bytes();
    let padding = usize::try_from(MAX_INPUT_BYTES).expect("the limit fits") + 1 - bytes.len();
    bytes.extend(std::iter::repeat_n(b' ', padding));
    std::fs::write(&path, &bytes).expect("the large file is written");
    let run = run_paths(&[&"transcript", &path]);
    assert_one_failure_line(&run, 2);
    assert!(run.stderr.contains("32 MiB input limit"), "{run:?}");

    let run = run_paths(&[
        &"transcript",
        &golden_path("sign-in.json"),
        &"--source",
        &path,
    ]);
    assert_one_failure_line(&run, 2);
    assert!(run.stderr.contains("the source file"), "{run:?}");
    assert!(run.stderr.contains("32 MiB input limit"), "{run:?}");
}

#[test]
fn an_input_at_the_limit_is_read() {
    let directory = TemporaryDirectory::new("at-limit");
    let path = directory.join("padded.json");
    let mut bytes = sign_in_bytes();
    let padding = usize::try_from(MAX_INPUT_BYTES).expect("the limit fits") - bytes.len();
    bytes.extend(std::iter::repeat_n(b' ', padding));
    std::fs::write(&path, &bytes).expect("the padded file is written");
    let run = run_paths(&[&"transcript", &path]);
    assert_eq!(run.code, 0, "{}", run.stderr);
}

#[test]
fn a_source_that_matches_no_snapshot_source_exits_2() {
    let directory = TemporaryDirectory::new("mismatch");
    let path = directory.join("sign-in.html");
    std::fs::write(&path, b"<p>another page</p>").expect("the page is written");
    let run = run_paths(&[
        &"transcript",
        &golden_path("sign-in.json"),
        &"--source",
        &path,
    ]);
    assert_one_failure_line(&run, 2);
    assert!(run.stderr.contains("matches no source"), "{run:?}");
    assert!(
        run.stderr
            .contains("may have changed since it was captured"),
        "{run:?}"
    );
}

#[test]
fn without_a_source_the_text_has_locations_and_no_excerpts() {
    let run = run_paths(&[&"transcript", &golden_path("sign-in.json")]);
    assert_eq!(run.code, 0, "{run:?}");
    assert!(
        run.stdout
            .lines()
            .any(|line| line.trim() == "sign-in.html:8:1"),
        "{}",
        run.stdout
    );
    assert!(!run.stdout.contains("<h1>"), "{}", run.stdout);

    let with_source = run_paths(&[
        &"transcript",
        &golden_path("sign-in.json"),
        &"--source",
        &page_of(Path::new("sign-in.json")),
    ]);
    assert!(
        with_source
            .stdout
            .lines()
            .any(|line| line.trim() == "sign-in.html:8:1  <h1>"),
        "{}",
        with_source.stdout
    );
}

#[test]
fn an_excerpt_collapses_whitespace_and_is_cut_at_80_characters() {
    // The heading's start tag widened to the whole `<html>` element, which spans many lines.
    let mut value = golden_value(Path::new("sign-in.json"));
    let html = value["spans"][0]["element"].clone();
    let heading = value["spans"]
        .as_array_mut()
        .expect("spans")
        .iter_mut()
        .find(|span| span["line"] == json!(8))
        .expect("the heading's span");
    heading["start_tag"] = html;
    let directory = TemporaryDirectory::new("excerpt");
    let path = directory.join("sign-in.json");
    std::fs::write(&path, value.to_string()).expect("the snapshot is written");
    let run = run_paths(&[
        &"transcript",
        &path,
        &"--source",
        &page_of(Path::new("sign-in.json")),
    ]);
    assert_eq!(run.code, 0, "{run:?}");
    let line = run
        .stdout
        .lines()
        .find(|line| line.trim_start().starts_with("sign-in.html:8:1"))
        .expect("the heading's location line");
    let excerpt = line
        .trim_start()
        .strip_prefix("sign-in.html:8:1  ")
        .expect("an excerpt");
    assert!(
        excerpt.starts_with("<html lang=\"en\"> <head> <meta charset=\"utf-8\"> <title>"),
        "{excerpt}"
    );
    assert!(excerpt.ends_with('…'), "{excerpt}");
    assert_eq!(excerpt.chars().count(), 81, "{excerpt}");
}

fn adapted_sign_in() -> spokenpath_model::Model {
    let snapshot = read(&String::from_utf8(sign_in_bytes()).expect("UTF-8")).expect("reads");
    let page = page_of(Path::new("sign-in.json"));
    let length = std::fs::metadata(page).expect("the page exists").len();
    adapt(&snapshot, &BTreeMap::from([(0, length)])).expect("adapts")
}

#[test]
fn json_output_is_the_serialised_transcript() {
    let page = page_of(Path::new("sign-in.json"));
    for (options, extra) in [
        (Options::default(), None),
        (Options { layer_b: false }, Some("--no-speech")),
    ] {
        let mut args: Vec<&dyn AsRef<std::ffi::OsStr>> = vec![&"transcript", &"--format", &"json"];
        let snapshot = golden_path("sign-in.json");
        args.push(&snapshot);
        args.push(&"--source");
        args.push(&page);
        if let Some(extra) = &extra {
            args.push(extra);
        }
        let run = run_paths(&args);
        assert_eq!(run.code, 0, "{run:?}");
        assert!(run.stdout.ends_with("}\n"), "{}", run.stdout);
        let expected = announce(&adapted_sign_in(), options);
        assert_eq!(
            run.stdout,
            serde_json::to_string_pretty(&expected).expect("serialises") + "\n"
        );
        let output: Value = serde_json::from_str(&run.stdout).expect("JSON");
        assert_eq!(output, serde_json::to_value(&expected).expect("serialises"));
        assert_eq!(
            run.stdout.contains("\"approx\""),
            options.layer_b,
            "{extra:?}"
        );
    }
}

/// A writer whose every write fails, as a closed pipe does.
struct ClosedPipe;

impl Write for ClosedPipe {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }
}

#[test]
fn a_failed_write_to_stdout_exits_1() {
    for format in ["text", "json"] {
        let mut stderr = Vec::new();
        let code = run(
            ["transcript", "-", "--format", format].map(OsString::from),
            &mut &sign_in_bytes()[..],
            &mut ClosedPipe,
            &mut stderr,
            "0.0.0",
        );
        assert_eq!(code, 1, "{format}");
        let stderr = String::from_utf8(stderr).expect("UTF-8");
        assert!(
            stderr.starts_with("spokenpath-core: cannot write the transcript"),
            "{stderr}"
        );
    }
    let mut stderr = Vec::new();
    let code = run(
        [OsString::from("--version")],
        &mut io::empty(),
        &mut ClosedPipe,
        &mut stderr,
        "0.0.0",
    );
    assert_eq!(code, 1);
}

#[test]
fn diagnostics_go_to_stderr_in_both_formats() {
    let mut value = golden_value(Path::new("sign-in.json"));
    value["diagnostics"] = json!([{ "code": "request-blocked", "message": "blocked a request" }]);
    let input = value.to_string();
    let plain = run_stdin(sign_in_bytes().as_slice(), &[]);
    for format in ["text", "json"] {
        let run = run_stdin(input.as_bytes(), &["--format", format]);
        assert_eq!(run.code, 0, "{run:?}");
        assert_eq!(
            run.stderr,
            "spokenpath-core: diagnostic request-blocked: blocked a request\n"
        );
        if format == "text" {
            assert_eq!(run.stdout, plain.stdout);
        }
    }
}

/// ESC with an SGR sequence, CSI, BEL, a bidi override, a zero-width space, a line separator, a
/// tag character and an invisible operator.
const HOSTILE: &str = "\u{1b}[31m\u{9b}2J\u{7}\u{202e}\u{200b}\u{2028}\u{e0041}\u{2062}";
const HOSTILE_RAW: [char; 8] = [
    '\u{1b}',
    '\u{9b}',
    '\u{7}',
    '\u{202e}',
    '\u{200b}',
    '\u{2028}',
    '\u{e0041}',
    '\u{2062}',
];
const HOSTILE_ESCAPED: [&str; 8] = [
    "\\u{1b}[31m",
    "\\u{9b}",
    "\\u{7}",
    "\\u{202e}",
    "\\u{200b}",
    "\\u{2028}",
    "\\u{e0041}",
    "\\u{2062}",
];

/// `names/aria-label.json` with [`HOSTILE`] in a name, a text run, an attribute value, the source
/// path and a diagnostic message.
fn hostile_capture() -> Value {
    let mut value = golden_value(Path::new("names/aria-label.json"));
    value["sources"][0]["path"] = json!(format!("aria{HOSTILE}label.html"));
    value["capture"]["engine_version"] = json!(format!("159{HOSTILE}"));
    value["diagnostics"] = json!([{
        "code": format!("code{HOSTILE}"),
        "message": format!("diagnostic {HOSTILE}"),
    }]);
    let ax = value["payload"]["ax"].as_array_mut().expect("ax nodes");
    let button = ax
        .iter_mut()
        .find(|node| node["role"]["value"] == json!("button"))
        .expect("a button");
    button["name"]["value"] = json!(format!("name {HOSTILE}"));
    let source = button["name"]["sources"]
        .as_array_mut()
        .expect("name sources")
        .iter_mut()
        .find(|source| source.get("attributeValue").is_some())
        .expect("an attribute source");
    source["attributeValue"]["value"] = json!(format!("attribute {HOSTILE}"));
    let text = ax
        .iter_mut()
        .find(|node| node["role"]["value"] == json!("StaticText"))
        .expect("a text node");
    text["name"]["value"] = json!(format!("text {HOSTILE}"));
    value
}

fn assert_escaped(output: &str, what: &str) {
    for raw in HOSTILE_RAW {
        assert!(
            !output.contains(raw),
            "{what} carries {raw:?} raw: {output}"
        );
    }
    for escaped in HOSTILE_ESCAPED {
        assert!(output.contains(escaped), "{what} lacks {escaped}: {output}");
    }
}

fn strings_in<'v>(value: &'v Value, strings: &mut Vec<&'v str>) {
    match value {
        Value::String(text) => strings.push(text),
        Value::Array(items) => items.iter().for_each(|item| strings_in(item, strings)),
        Value::Object(object) => object.values().for_each(|item| strings_in(item, strings)),
        _ => {}
    }
}

#[test]
fn hostile_strings_reach_the_terminal_only_escaped_and_json_keeps_them_raw() {
    let directory = TemporaryDirectory::new("hostile");
    // The page with a hostile start tag appended, which the first span is pointed at, so its
    // excerpt carries the hostile bytes; the snapshot's source hash is the new file's.
    let mut page_bytes =
        std::fs::read(page_of(Path::new("names/aria-label.json"))).expect("the page is readable");
    let start = page_bytes.len();
    page_bytes.extend(format!("<b title=\"excerpt {HOSTILE}\">").bytes());
    let range = json!({ "start": start, "end": page_bytes.len() });
    let page = directory.join("hostile.html");
    std::fs::write(&page, &page_bytes).expect("the page is written");
    let mut capture = hostile_capture();
    capture["sources"][0]["sha256"] = json!(sha256_hex(&page_bytes));
    capture["spans"][0]["start_tag"] = range.clone();
    capture["spans"][0]["element"] = range;
    let path = directory.join("hostile.json");
    std::fs::write(&path, capture.to_string()).expect("the capture is written");

    let text = run_paths(&[&"transcript", &path, &"--source", &page]);
    assert_eq!(text.code, 0, "{text:?}");
    assert_escaped(&text.stdout, "the text output");
    assert_escaped(&text.stderr, "stderr");
    for expected in ["name ", "text ", "attribute ", "aria", "excerpt ", "159"] {
        assert!(
            text.stdout.contains(&format!("{expected}\\u{{1b}}[31m")),
            "{expected}: {}",
            text.stdout
        );
    }
    // The excerpt still comes from the page, under the hostile path.
    assert!(text.stdout.contains("label.html:"), "{}", text.stdout);
    assert!(
        text.stderr.contains("diagnostic code\\u{1b}[31m"),
        "{}",
        text.stderr
    );

    let json = run_paths(&[
        &"transcript",
        &path,
        &"--source",
        &page,
        &"--format",
        &"json",
    ]);
    assert_eq!(json.code, 0, "{json:?}");
    assert_escaped(&json.stderr, "stderr");
    let value: Value = serde_json::from_str(&json.stdout).expect("JSON");
    let mut strings = Vec::new();
    strings_in(&value, &mut strings);
    for raw in [
        format!("name {HOSTILE}"),
        format!("text {HOSTILE}"),
        format!("attribute {HOSTILE}"),
        format!("aria{HOSTILE}label.html"),
        format!("159{HOSTILE}"),
    ] {
        assert!(strings.contains(&raw.as_str()), "JSON keeps {raw:?} raw");
    }

    // A path echoed in an error message is escaped too.
    let missing = directory.join(format!("missing{HOSTILE}.json"));
    let error = run_paths(&[&"transcript", &missing]);
    assert_eq!(error.code, 2, "{error:?}");
    assert_escaped(&error.stderr, "an error message");
}

#[test]
fn a_line_break_in_a_text_run_shows_as_its_escape() {
    let mut value = golden_value(Path::new("sign-in.json"));
    let text = value["payload"]["ax"]
        .as_array_mut()
        .expect("ax nodes")
        .iter_mut()
        .find(|node| {
            node["name"]["value"] == json!("Email") && node["role"]["value"] == json!("StaticText")
        })
        .expect("the label's text");
    text["name"]["value"] = json!("Line one\nLine two");
    let run = run_stdin(value.to_string().as_bytes(), &[]);
    assert_eq!(run.code, 0, "{run:?}");
    let lines: Vec<&str> = run.stdout.lines().map(str::trim).collect();
    assert!(
        lines.contains(&"6        A text \"Line one\\u{a}Line two\""),
        "{}",
        run.stdout
    );
    assert!(
        lines.contains(&"~ \"Line one\\u{a}Line two\""),
        "{}",
        run.stdout
    );
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, byte| {
            // Writing to a String cannot fail.
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// The marker every string leaf of a poisoned transcript carries: ESC with an SGR sequence, CSI,
/// a bidi override, a tag character and an invisible operator.
const MARK: &str = "\u{1b}[31m\u{9b}\u{202e}\u{e0041}\u{2062}";
const MARK_RAW: [char; 5] = ['\u{1b}', '\u{9b}', '\u{202e}', '\u{e0041}', '\u{2062}'];
const MARK_ESCAPED: &str = "\\u{1b}[31m\\u{9b}\\u{202e}\\u{e0041}\\u{2062}";

/// A start tag whose excerpt carries the marker; every span of a poisoned transcript points at it.
fn marked_start_tag() -> String {
    format!("<p title=\"{MARK}\">")
}

fn mark(text: &mut String) {
    text.push_str(MARK);
}

/// `text` with the marker, as the `&'static str` some transcript fields hold.
fn marked(text: &str) -> &'static str {
    Box::leak(format!("{text}{MARK}").into_boxed_str())
}

fn mark_dom(dom: &mut spokenpath_announce::DomRecord) {
    if let Some(tag) = &mut dom.tag {
        mark(tag);
    }
    let id = dom.id.get_or_insert_with(String::new);
    mark(id);
}

/// A span keeps its class, with a marked path and the marked start tag; any other class becomes
/// one the printer does not know.
fn mark_source(source: &mut spokenpath_announce::SourceRecord) {
    if source.class == "span" {
        if let Some(path) = &mut source.path {
            mark(path);
        }
        source.start_tag = Some(spokenpath_announce::ByteRangeRecord {
            start: 0,
            end: marked_start_tag().len() as u64,
        });
    } else {
        source.class = marked(source.class);
    }
}

fn mark_related(node: &mut spokenpath_announce::RelatedRecord) {
    if let Some(dom) = &mut node.dom {
        mark_dom(dom);
    }
    for text in [&mut node.idref, &mut node.text].into_iter().flatten() {
        mark(text);
    }
    if let Some(source) = &mut node.source {
        mark_source(source);
    }
}

/// A related node known only by its idref, whose text and idref both carry the marker.
fn marked_idref() -> spokenpath_announce::RelatedRecord {
    spokenpath_announce::RelatedRecord {
        dom: None,
        idref: Some(MARK.to_owned()),
        text: Some(MARK.to_owned()),
        source: None,
    }
}

fn mark_name(name: &mut spokenpath_announce::NameRecord) {
    use spokenpath_announce::NameSourceOutcome;
    if let Some(text) = &mut name.text {
        mark(text);
    }
    for (index, source) in name.sources.iter_mut().flatten().enumerate() {
        mark(&mut source.kind);
        for text in [
            &mut source.attribute,
            &mut source.native_source,
            &mut source.value,
        ]
        .into_iter()
        .flatten()
        {
            mark(text);
        }
        // The first source shows an attribute with its value, the others an attribute alone.
        if index == 0 && source.attribute.is_some() {
            source.attribute_value = Some(MARK.to_owned());
        }
        source.related_nodes.iter_mut().for_each(mark_related);
        match source.outcome {
            NameSourceOutcome::Used => {
                source.related_nodes.push(marked_idref());
                source
                    .related_nodes
                    .push(spokenpath_announce::RelatedRecord {
                        dom: None,
                        idref: Some("script-made".to_owned()),
                        text: None,
                        source: Some(spokenpath_announce::SourceRecord {
                            class: marked("script"),
                            path: None,
                            line: None,
                            column: None,
                            start_tag: None,
                        }),
                    });
            }
            NameSourceOutcome::Empty => {
                source.outcome = NameSourceOutcome::Invalid;
                source.invalid_reason = Some(MARK.to_owned());
            }
            NameSourceOutcome::Superseded | NameSourceOutcome::Invalid => {}
        }
    }
}

/// The sign-in transcript with the marker in every string the text printer may write, except the
/// Layer B phrases and the tab-order ending, which the announce crate has already escaped. Every
/// accessible item gains a value, an `invalid` token and a description, every winning name source
/// gains related nodes known only by idref, one tab stop loses its item so it prints as an element
/// and another cites a text run, and every span points at [`marked_start_tag`].
fn poisoned_transcript() -> spokenpath_announce::Transcript {
    use spokenpath_announce::{
        DescriptionRecord, DescriptionSourceRecord, LayerA, Marked, Marking, Stop,
    };
    let mut transcript = announce(&adapted_sign_in(), Options::default());
    transcript.header.engine = marked(transcript.header.engine);
    mark(&mut transcript.header.engine_version);
    for item in &mut transcript.nodes {
        match &mut item.layer_a {
            LayerA::TextRun(run) => mark(&mut run.text),
            LayerA::Accessible(record) => {
                if let Some(role) = &mut record.role {
                    mark(&mut role.token);
                }
                if let Some(dom) = &mut record.dom {
                    mark_dom(dom);
                }
                mark_source(&mut record.source);
                mark_name(&mut record.name);
                record.value = Some(MARK.to_owned());
                record.states.invalid = Some(format!("token{MARK}"));
                let mut element = marked_idref();
                element.dom = record.dom.clone();
                record.description = Some(DescriptionRecord {
                    text: MARK.to_owned(),
                    provenance: Some(Marked {
                        value: DescriptionSourceRecord::AriaDescribedby {
                            elements: vec![element],
                        },
                        by: Marking::ComputedByTool,
                    }),
                });
            }
        }
    }
    for excluded in transcript.excluded.iter_mut().flatten() {
        mark_dom(&mut excluded.dom);
        mark_source(&mut excluded.source);
        for reason in &mut excluded.reasons {
            reason.kind = marked(reason.kind);
            mark(&mut reason.engine_reason);
            if let Some(ancestor) = &mut reason.hiding_ancestor {
                mark_dom(&mut ancestor.value.dom);
                if let Some(source) = &mut ancestor.value.source {
                    mark_source(source);
                }
            }
        }
    }
    let indexes = &mut transcript.indexes;
    let entries = indexes
        .headings
        .iter_mut()
        .map(|heading| &mut heading.entry)
        .chain(indexes.landmarks.iter_mut())
        .chain(indexes.links.iter_mut())
        .chain(indexes.form_controls.iter_mut());
    for entry in entries {
        mark(&mut entry.role);
        if let Some(name) = &mut entry.name {
            mark(name);
        }
    }
    for (index, step) in transcript
        .tab_order
        .iter_mut()
        .flat_map(|tab_order| tab_order.steps.iter_mut())
        .enumerate()
    {
        if let Stop::Element { dom, source, item } = &mut step.stop {
            mark_dom(dom);
            if let Some(source) = source {
                mark_source(source);
            }
            match index {
                1 => *item = None,
                // A stop that cites a text run, which the printer quotes as one.
                2 => *item = Some(3),
                _ => {}
            }
        }
    }
    transcript
}

#[test]
fn every_page_controlled_string_in_the_text_is_escaped() {
    let transcript = poisoned_transcript();
    let source = crate::text::SourceText {
        paths: [format!("sign-in.html{MARK}")].into(),
        bytes: marked_start_tag().into_bytes(),
    };
    let context = crate::text::TextContext {
        tool_version: "0.0.0",
        snapshot_version: (0, 0),
        viewport: (1280, 720),
        layer_b: true,
        source: Some(&source),
        aligned: None,
    };
    let mut out = Vec::new();
    crate::text::write_text(&mut out, &transcript, &context).expect("writes to a vector");
    let text = String::from_utf8(out).expect("UTF-8");
    for raw in MARK_RAW {
        assert!(!text.contains(raw), "{raw:?} reaches the text raw:\n{text}");
    }
    let e = MARK_ESCAPED;
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let expected = [
        // The header's engine token and version.
        format!("chromium{e} 159.0.8154.6{e} · viewport"),
        // An internal and an ARIA role token, a name, a value, the invalid token.
        format!("A RootWebArea{e} (internal role) \"Sign in{e}\"  value \"{e}\""),
        format!("A heading{e} \"Sign in{e}\"  value \"{e}\"  invalid (token{e})"),
        // A text run.
        format!("A text \"Sign in{e}\""),
        // A span's path and its excerpt.
        format!("sign-in.html{e}:8:1  <p title=\"{e}\">"),
        // A class the printer does not know.
        format!("no source location: (document{e})"),
        // The winning source's kind and native source.
        format!("name from title{e} (related-element{e})"),
        // An attribute and its value, an invalid reason.
        format!(
            "aria-labelledby{e}=\"{e}\" (related-element{e}): invalid ({e}); aria-label{e} (attribute{e}): invalid ({e})"
        ),
        // A related node's tag, id, text and location.
        format!("via <label{e}> #{e}  \"Email{e}\"  sign-in.html{e}:10:1"),
        // A related node's idref and text.
        format!("via idref \"{e}\"  \"{e}\"  no source location held"),
        // A related node whose source is not a span.
        format!("via idref \"script-made\"  no source location (script{e})"),
        // A description and its element.
        format!("description \"{e}\"  from aria-describedby: <h1{e}> #{e}  \"{e}\""),
        // An excluded element, its kind and engine reason, and its hiding ancestor.
        format!("<meta{e}> #{e}  sign-in.html{e}:4:1  <p title=\"{e}\">"),
        format!(
            "not-rendered{e} (engine reason: notRendered{e}); hidden by <head{e}> #{e}  sign-in.html{e}:3:1 (computed by tool)"
        ),
        // Index entries: role and name.
        format!("[2] heading{e} \"Sign in{e}\"  level 1"),
        format!("[10] button{e} (no accessible name)"),
        // A tab stop cited by item, and one printed as an element.
        format!("1  [7] textbox{e} \"Email{e}\""),
        format!("3  [3] text \"Sign in{e}\""),
        format!(
            "2  <button{e}> #submit{e}  sign-in.html{e}:14:1  <p title=\"{e}\">  (not an item)"
        ),
    ];
    for line in &expected {
        assert!(
            lines.iter().any(|printed| printed.contains(line.as_str())),
            "no line holds {line}:\n{text}"
        );
    }
}

/// `sign-in.json` with its tree replaced by a chain of `depth` nested generic elements under the
/// root.
fn chain_snapshot(depth: u32) -> Value {
    let mut value = golden_value(Path::new("sign-in.json"));
    value["spans"] = json!([]);
    value["diagnostics"] = json!([]);
    let mut dom = vec![json!({
        "id": 0, "backend_id": 1, "children": [1], "kind": "document", "attributes": []
    })];
    let mut ax = vec![json!({
        "nodeId": "0", "ignored": false, "role": {"type": "internalRole", "value": "RootWebArea"},
        "childIds": ["1"], "backendDOMNodeId": 1
    })];
    for id in 1..=depth {
        let last = id == depth;
        dom.push(json!({
            "id": id, "backend_id": id + 1, "parent": id - 1,
            "children": if last { json!([]) } else { json!([id + 1]) },
            "kind": "element", "tag": "div", "attributes": [], "origin": "script"
        }));
        ax.push(json!({
            "nodeId": id.to_string(), "ignored": false, "role": {"type": "role", "value": "generic"},
            "parentId": (id - 1).to_string(),
            "childIds": if last { json!([]) } else { json!([(id + 1).to_string()]) },
            "backendDOMNodeId": id + 1
        }));
    }
    let payload = value["payload"].as_object_mut().expect("a payload");
    payload.insert("dom".to_owned(), Value::Array(dom));
    payload.insert("ax".to_owned(), Value::Array(ax));
    payload.insert("ax_forced".to_owned(), json!([]));
    payload.insert("focus".to_owned(), json!({"steps": [], "truncated": false}));
    payload.remove("dom_snapshot");
    value
}

#[test]
fn items_deeper_than_the_indent_cap_state_their_depth_and_are_indented_no_further() {
    let run = run_stdin(chain_snapshot(200).to_string().as_bytes(), &[]);
    assert_eq!(run.code, 0, "{run:?}");
    let lines: Vec<&str> = run.stdout.lines().collect();
    // Item k + 1 is the chain element at depth k.
    let item = |number: usize| {
        lines
            .iter()
            .find(|line| line.trim_start().starts_with(&format!("{number}  ")))
            .copied()
            .unwrap_or_else(|| panic!("no item {number}:\n{}", run.stdout))
    };
    assert!(!item(33).contains("[depth"), "{}", item(33));
    assert!(
        item(34).contains("[depth 33] A generic (no accessible name)"),
        "{}",
        item(34)
    );
    assert!(
        item(201).contains("[depth 200] A generic (no accessible name)"),
        "{}",
        item(201)
    );
    // The number column (3), two spaces, 32 levels of two spaces, and the detail indent (2).
    let widest = 3 + 2 + 2 * 32 + 2;
    for line in &lines {
        let content = line
            .trim_start()
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .trim_start();
        let before_content = line.len() - content.len();
        assert!(
            before_content <= widest,
            "{before_content} columns before {line:?}"
        );
    }
}

#[test]
fn no_excerpt_is_printed_for_a_path_that_an_unmatched_source_shares() {
    let mut value = golden_value(Path::new("sign-in.json"));
    value["sources"]
        .as_array_mut()
        .expect("sources")
        .push(json!({
            "id": 1,
            "path": "sign-in.html",
            "sha256": "f".repeat(64),
            "encoding": "utf-8"
        }));
    let heading = value["spans"]
        .as_array_mut()
        .expect("spans")
        .iter_mut()
        .find(|span| span["line"] == json!(8))
        .expect("the heading's span");
    heading["source"] = json!(1);
    let directory = TemporaryDirectory::new("shared-path");
    let path = directory.join("sign-in.json");
    std::fs::write(&path, value.to_string()).expect("the snapshot is written");
    let run = run_paths(&[
        &"transcript",
        &path,
        &"--source",
        &page_of(Path::new("sign-in.json")),
    ]);
    assert_eq!(run.code, 0, "{run:?}");
    let lines: Vec<&str> = run.stdout.lines().map(str::trim).collect();
    assert!(lines.contains(&"sign-in.html:8:1"), "{}", run.stdout);
    assert!(!run.stdout.contains("<h1>"), "{}", run.stdout);
    // The shared path is ambiguous for every span on it, so none borrows the file's bytes.
    assert!(lines.contains(&"sign-in.html:11:1"), "{}", run.stdout);
}

fn safari_goldens_directory() -> PathBuf {
    fixtures().join("snapshots/safari-webdriver")
}

/// Every Safari capture under `fixtures/snapshots/safari-webdriver/`, relative to it, found by
/// listing the directories, so a new capture without a cross-check text golden fails the test.
fn safari_golden_names() -> Vec<PathBuf> {
    let root = safari_goldens_directory();
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

#[test]
fn every_safari_capture_matches_its_cross_check_text_golden() {
    let names = safari_golden_names();
    assert!(
        names
            .iter()
            .any(|name| name.starts_with("cross-check") && name.ends_with("list-style-none.json")),
        "{names:?}"
    );
    assert_eq!(
        names,
        golden_names()
            .into_iter()
            .filter(|name| names.contains(name))
            .collect::<Vec<_>>(),
        "every Safari capture has a Chromium capture at the same relative path"
    );
    for name in &names {
        let run = run_paths(&[
            &"transcript",
            &goldens_directory().join(name),
            &"--source",
            &page_of(name),
            &"--cross-check",
            &safari_goldens_directory().join(name),
        ]);
        assert_eq!(run.code, 0, "{}: {run:?}", name.display());
        insta::assert_snapshot!(
            format!("safari-cross-check__{}", snapshot_name(name)),
            run.stdout
        );
    }
}

fn basic_pair() -> (PathBuf, PathBuf, PathBuf) {
    let snapshots = fixtures().join("snapshots");
    (
        snapshots.join("chromium-basic.json"),
        snapshots.join("safari-webdriver-basic.json"),
        snapshots.join("pages/basic.html"),
    )
}

fn fixture_value(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is JSON: {error}", path.display()))
}

#[test]
fn the_hand_written_pair_matches_its_cross_check_goldens() {
    let (chromium, safari, page) = basic_pair();
    let text = run_paths(&[
        &"transcript",
        &chromium,
        &"--source",
        &page,
        &"--cross-check",
        &safari,
    ]);
    assert_eq!(text.code, 0, "{text:?}");
    for line in [
        "cross-check · Safari 27.0.1 on macOS 27.0.1 (26A434) over WebDriver · role and name of 4 elements compared: 2 differ, 2 not checked",
        "differs in Safari: role \"\" in Safari 27.0.1, \"LabelText\" in Chromium 159.0.8154.6",
        "not checked in Safari 27.0.1: role (Safari answered with error \"unknown error\")",
        "not checked in Safari 27.0.1: role and name (no element with this tag was found in Safari)",
    ] {
        assert!(
            text.stdout.lines().any(|printed| printed.trim() == line),
            "no line {line}:\n{}",
            text.stdout
        );
    }
    insta::assert_snapshot!("safari-cross-check__basic", text.stdout);

    let json = run_paths(&[
        &"transcript",
        &chromium,
        &"--cross-check",
        &safari,
        &"--format",
        &"json",
    ]);
    assert_eq!(json.code, 0, "{json:?}");
    let value: Value = serde_json::from_str(&json.stdout).expect("JSON");
    let items: Vec<Value> = value["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .filter(|item| item.get("cross_check").is_some())
        .map(|item| json!({ "number": item["number"], "cross_check": item["cross_check"] }))
        .collect();
    let fields = json!({ "cross_check": value["cross_check"], "items": items });
    insta::assert_snapshot!(
        "safari-cross-check__basic__json",
        serde_json::to_string_pretty(&fields).expect("serialises")
    );
}

#[test]
fn the_aria_hidden_paragraph_chromium_excludes_gets_no_note() {
    let (chromium, safari, page) = basic_pair();
    let run = run_paths(&[
        &"transcript",
        &chromium,
        &"--source",
        &page,
        &"--cross-check",
        &safari,
    ]);
    assert_eq!(run.code, 0, "{run:?}");
    assert!(
        !run.stdout.contains("\"paragraph\""),
        "Safari's answer for the paragraph is printed:\n{}",
        run.stdout
    );
}

#[test]
fn notes_print_without_speech() {
    let (chromium, safari, _) = basic_pair();
    let run = run_paths(&[
        &"transcript",
        &chromium,
        &"--cross-check",
        &safari,
        &"--no-speech",
    ]);
    assert_eq!(run.code, 0, "{run:?}");
    assert!(!run.stdout.contains('~'), "{}", run.stdout);
    assert_eq!(run.stdout.matches("differs in Safari: ").count(), 2);
    assert_eq!(run.stdout.matches("not checked in Safari ").count(), 3);
}

#[test]
fn the_cross_check_snapshot_can_come_from_stdin() {
    let (chromium, safari, _) = basic_pair();
    let from_file = run_paths(&[&"transcript", &chromium, &"--cross-check", &safari]);
    let bytes = std::fs::read(&safari).expect("readable");
    let args: Vec<OsString> = ["transcript", "--cross-check", "-"]
        .iter()
        .map(OsString::from)
        .chain([chromium.into_os_string()])
        .collect();
    let from_stdin = run_with(&args.iter().collect::<Vec<_>>(), &mut &bytes[..]);
    assert_eq!(from_stdin.code, 0, "{from_stdin:?}");
    assert_eq!(from_stdin.stdout, from_file.stdout);
}

#[test]
fn a_cross_check_from_another_page_exits_2_with_one_line() {
    let (_, safari, _) = basic_pair();
    let run = run_paths(&[
        &"transcript",
        &golden_path("sign-in.json"),
        &"--cross-check",
        &safari,
    ]);
    assert_one_failure_line(&run, 2);
    assert_eq!(
        run.stderr,
        "spokenpath-core: the cross-check snapshot was captured from a different page: its sources or spans differ from the snapshot's\n"
    );
}

#[test]
fn a_chromium_cross_check_snapshot_exits_2_naming_its_engine() {
    let run = run_paths(&[
        &"transcript",
        &golden_path("sign-in.json"),
        &"--cross-check",
        &golden_path("sign-in.json"),
    ]);
    assert_one_failure_line(&run, 2);
    assert_eq!(run.stderr.lines().count(), 1, "{run:?}");
    assert!(run.stderr.contains("the cross-check snapshot: "), "{run:?}");
    assert!(run.stderr.contains("`chromium`"), "{run:?}");
}

#[test]
fn an_oversized_cross_check_exits_2_without_parsing() {
    let directory = TemporaryDirectory::new("oversized-cross-check");
    let path = directory.join("large.json");
    let (chromium, safari, _) = basic_pair();
    let mut bytes = std::fs::read(safari).expect("readable");
    let padding = usize::try_from(MAX_INPUT_BYTES).expect("the limit fits") + 1 - bytes.len();
    bytes.extend(std::iter::repeat_n(b' ', padding));
    std::fs::write(&path, &bytes).expect("the large file is written");
    let run = run_paths(&[&"transcript", &chromium, &"--cross-check", &path]);
    assert_one_failure_line(&run, 2);
    assert!(run.stderr.contains("the cross-check snapshot"), "{run:?}");
    assert!(run.stderr.contains("32 MiB input limit"), "{run:?}");
}

#[test]
fn a_major_one_cross_check_exits_3() {
    let directory = TemporaryDirectory::new("major-cross-check");
    let path = directory.join("major.json");
    let (chromium, safari, _) = basic_pair();
    let mut value = fixture_value(&safari);
    value["version"]["major"] = json!(1);
    std::fs::write(&path, value.to_string()).expect("written");
    let run = run_paths(&[&"transcript", &chromium, &"--cross-check", &path]);
    assert_one_failure_line(&run, 3);
    assert!(
        run.stderr
            .starts_with("spokenpath-core: the cross-check snapshot: snapshot major version 1"),
        "{run:?}"
    );
}

#[test]
fn cross_check_usage_errors_exit_2_with_the_usage_line() {
    let (chromium, safari, _) = basic_pair();
    let (chromium, safari) = (
        chromium.to_str().expect("UTF-8").to_owned(),
        safari.to_str().expect("UTF-8").to_owned(),
    );
    for (case, args, says) in [
        (
            "two snapshots on stdin",
            vec!["transcript", "-", "--cross-check", "-"],
            "cannot both be `-`",
        ),
        (
            "a repeated --cross-check",
            vec![
                "transcript",
                &chromium,
                "--cross-check",
                &safari,
                "--cross-check",
                &safari,
            ],
            "`--cross-check` is given more than once",
        ),
        (
            "a missing value",
            vec!["transcript", &chromium, "--cross-check"],
            "`--cross-check` needs a value",
        ),
    ] {
        let run = run_args(&args);
        assert_one_failure_line(&run, 2);
        assert!(run.stderr.contains(says), "{case}: {run:?}");
        assert!(
            run.stderr.contains("[--cross-check <snapshot | ->]"),
            "{case}: {run:?}"
        );
    }
}

#[test]
fn cross_check_diagnostics_go_to_stderr_naming_their_engine() {
    let directory = TemporaryDirectory::new("cross-check-diagnostics");
    let path = directory.join("safari.json");
    let (chromium, safari, _) = basic_pair();
    let mut value = fixture_value(&safari);
    value["diagnostics"] = json!([{ "code": "asset-not-found", "message": "a 404" }]);
    std::fs::write(&path, value.to_string()).expect("written");
    let run = run_paths(&[&"transcript", &chromium, &"--cross-check", &path]);
    assert_eq!(run.code, 0, "{run:?}");
    assert!(
        run.stderr
            .lines()
            .any(|line| line
                == "spokenpath-core: diagnostic safari-webdriver asset-not-found: a 404"),
        "{run:?}"
    );
}

/// The Chromium and Safari snapshots of the hand-written pair, as JSON values to poison.
fn poisonable_pair() -> (Value, Value) {
    let (chromium, safari, _) = basic_pair();
    (fixture_value(&chromium), fixture_value(&safari))
}

fn safari_element(safari: &mut Value, span: u32) -> &mut Value {
    safari["payload"]["elements"]
        .as_array_mut()
        .expect("elements")
        .iter_mut()
        .find(|element| element["span"] == json!(span))
        .expect("an element for the span")
}

fn chromium_ax<'v>(chromium: &'v mut Value, role: &str) -> &'v mut Value {
    chromium["payload"]["ax"]
        .as_array_mut()
        .expect("ax nodes")
        .iter_mut()
        .find(|node| node["role"]["value"] == json!(role))
        .expect("an AX node with the role")
}

fn append(value: &mut Value) {
    let text = value.as_str().expect("a string").to_owned();
    *value = json!(format!("{text}{MARK}"));
}

#[test]
fn every_cross_check_string_in_the_text_is_escaped_and_json_keeps_it_raw() {
    type Poison = fn(&mut Value, &mut Value);
    let fields: [(&str, Poison); 12] = [
        ("Safari's role", |_, safari| {
            append(&mut safari_element(safari, 2)["role"]);
        }),
        ("Safari's label", |_, safari| {
            append(&mut safari_element(safari, 1)["label"]);
        }),
        ("Chromium's role beside Safari's", |chromium, _| {
            append(&mut chromium_ax(chromium, "LabelText")["role"]["value"]);
        }),
        ("Chromium's name beside Safari's", |chromium, _| {
            append(&mut chromium_ax(chromium, "heading")["name"]["value"]);
        }),
        ("the error code", |_, safari| {
            append(&mut safari_element(safari, 3)["role_error"]["error"]);
        }),
        ("the error message", |_, safari| {
            append(&mut safari_element(safari, 3)["label_error"]["message"]);
        }),
        ("Safari's version", |_, safari| {
            append(&mut safari["capture"]["engine_version"]);
        }),
        ("Chromium's version", |chromium, _| {
            append(&mut chromium["capture"]["engine_version"]);
        }),
        ("the macOS version", |_, safari| {
            append(&mut safari["payload"]["macos_version"]);
        }),
        ("the macOS build", |_, safari| {
            append(&mut safari["payload"]["macos_build"]);
        }),
        ("a diagnostic code", |_, safari| {
            safari["diagnostics"] = json!([{ "code": format!("code{MARK}"), "message": "m" }]);
        }),
        ("a diagnostic message", |_, safari| {
            safari["diagnostics"] = json!([{ "code": "code", "message": format!("m{MARK}") }]);
        }),
    ];
    let directory = TemporaryDirectory::new("hostile-cross-check");
    for (field, poison) in fields {
        let (mut chromium, mut safari) = poisonable_pair();
        poison(&mut chromium, &mut safari);
        let chromium_path = directory.join("chromium.json");
        let safari_path = directory.join("safari.json");
        std::fs::write(&chromium_path, chromium.to_string()).expect("written");
        std::fs::write(&safari_path, safari.to_string()).expect("written");

        let text = run_paths(&[
            &"transcript",
            &chromium_path,
            &"--cross-check",
            &safari_path,
        ]);
        assert_eq!(text.code, 0, "{field}: {text:?}");
        let printed = format!("{}{}", text.stdout, text.stderr);
        for raw in MARK_RAW {
            assert!(
                !printed.contains(raw),
                "{field} reaches the output with {raw:?} raw:\n{printed}"
            );
        }
        assert!(
            printed.contains(MARK_ESCAPED),
            "{field} is not printed escaped:\n{printed}"
        );

        let json = run_paths(&[
            &"transcript",
            &chromium_path,
            &"--cross-check",
            &safari_path,
            &"--format",
            &"json",
        ]);
        assert_eq!(json.code, 0, "{field}: {json:?}");
        for raw in MARK_RAW {
            assert!(
                !json.stderr.contains(raw),
                "{field} reaches stderr with {raw:?} raw"
            );
        }
        if !field.starts_with("a diagnostic") {
            assert!(
                json.stdout.contains("\\u001b[31m"),
                "{field}: JSON keeps it raw"
            );
        }
    }
}
