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
    value["diagnostics"] =
        json!([{ "code": "hostile", "message": format!("diagnostic {HOSTILE}") }]);
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
    let path = directory.join("hostile.json");
    std::fs::write(&path, hostile_capture().to_string()).expect("the capture is written");
    let page = page_of(Path::new("names/aria-label.json"));

    let text = run_paths(&[&"transcript", &path, &"--source", &page]);
    assert_eq!(text.code, 0, "{text:?}");
    assert_escaped(&text.stdout, "the text output");
    assert_escaped(&text.stderr, "stderr");
    for expected in ["name ", "text ", "attribute ", "aria"] {
        assert!(
            text.stdout.contains(&format!("{expected}\\u{{1b}}[31m")),
            "{expected}: {}",
            text.stdout
        );
    }
    // The excerpt still comes from the page, under the hostile path.
    assert!(text.stdout.contains("label.html:"), "{}", text.stdout);

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
