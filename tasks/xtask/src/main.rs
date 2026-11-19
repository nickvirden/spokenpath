//! Repository automation, run as `cargo xtask <command>`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Each command's name and the one-line description that `help` prints.
const COMMANDS: &[(&str, &str)] = &[
    ("help", "List the available commands"),
    (
        "schema",
        "Write schema/snapshot.v0.json from the snapshot types and schema/transcript.v0.json from \
         the transcript types; --check compares them instead",
    ),
];

/// One committed JSON Schema: its path relative to the repository root, the types it is
/// generated from, and the generator.
struct SchemaFile {
    path: &'static str,
    types: &'static str,
    generate: fn() -> serde_json::Value,
}

/// Every committed JSON Schema.
const SCHEMAS: [SchemaFile; 2] = [
    SchemaFile {
        path: "schema/snapshot.v0.json",
        types: "snapshot",
        generate: spokenpath_snapshot::json_schema,
    },
    SchemaFile {
        path: "schema/transcript.v0.json",
        types: "transcript",
        generate: spokenpath_announce::json_schema,
    },
];

fn help() -> String {
    let commands: Vec<String> = COMMANDS
        .iter()
        .map(|(name, description)| format!("  {name:<8}{description}"))
        .collect();
    format!(
        "Usage: cargo xtask <command>\n\nCommands:\n{}\n",
        commands.join("\n")
    )
}

/// The repository root, found from this crate's manifest so the command works from any directory.
fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The schema file's contents: pretty-printed with 2-space indent and a trailing newline.
fn generated_schema(schema: &SchemaFile) -> Result<String, String> {
    serde_json::to_string_pretty(&(schema.generate)())
        .map(|text| text + "\n")
        .map_err(|error| format!("could not serialise the {} schema: {error}", schema.types))
}

/// Writes `contents` to `schema`'s path under `root`, creating its directory.
fn write_schema(root: &Path, schema: &SchemaFile, contents: &str) -> Result<String, String> {
    let path = root.join(schema.path);
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .map_err(|error| format!("could not create {}: {error}", directory.display()))?;
    }
    std::fs::write(&path, contents)
        .map_err(|error| format!("could not write {}: {error}", schema.path))?;
    Ok(format!("wrote {}\n", schema.path))
}

/// Succeeds when `schema`'s file under `root` is byte-identical to `contents`, and writes nothing.
fn check_schema(root: &Path, schema: &SchemaFile, contents: &str) -> Result<String, String> {
    let SchemaFile { path, types, .. } = schema;
    match std::fs::read(root.join(path)) {
        Ok(committed) if committed == contents.as_bytes() => Ok(format!("{path} is up to date\n")),
        Ok(_) => Err(format!(
            "{path} differs from the schema the {types} types generate; run `cargo xtask schema` and commit the result"
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "{path} is missing; run `cargo xtask schema` and commit the result"
        )),
        Err(error) => Err(format!("could not read {path}: {error}")),
    }
}

/// Runs `schema` against the files under `root`: writes every schema, or with `--check` compares
/// every one and reports each that differs.
fn schema(flags: &[&str], root: &Path) -> Result<String, String> {
    let mut check = false;
    for flag in flags {
        match *flag {
            "--check" => check = true,
            unknown => {
                return Err(format!(
                    "unknown flag `{unknown}` for `schema`; run `cargo xtask help` to list the commands"
                ));
            }
        }
    }
    let mut output = String::new();
    let mut failures = Vec::new();
    for file in &SCHEMAS {
        let contents = generated_schema(file)?;
        let result = if check {
            check_schema(root, file, &contents)
        } else {
            write_schema(root, file, &contents)
        };
        match result {
            Ok(line) => output.push_str(&line),
            Err(message) => failures.push(message),
        }
    }
    if failures.is_empty() {
        Ok(output)
    } else {
        Err(failures.join("\nerror: "))
    }
}

/// Runs `command` with its `flags` and returns what to print, or an error message when the
/// command is unknown or fails. No command at all prints the help.
fn run(command: Option<&str>, flags: &[&str]) -> Result<String, String> {
    match command {
        None | Some("help") => Ok(help()),
        Some("schema") => schema(flags, &repository_root()),
        Some(unknown) => Err(format!(
            "unknown command `{unknown}`; run `cargo xtask help` to list the commands"
        )),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flags: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
    match run(args.first().map(String::as_str), &flags) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{SCHEMAS, generated_schema, run, schema};

    /// A fresh directory under the system temporary directory, unique to this process and test,
    /// removed when the value drops.
    struct TemporaryDirectory(PathBuf);

    impl TemporaryDirectory {
        fn new(name: &str) -> Self {
            let directory =
                std::env::temp_dir().join(format!("xtask-{}-{name}", std::process::id()));
            if directory.exists() {
                std::fs::remove_dir_all(&directory).expect("stale test directory is removed");
            }
            std::fs::create_dir_all(&directory).expect("test directory is created");
            Self(directory)
        }

        fn path(&self) -> &Path {
            &self.0
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

    #[test]
    fn help_lists_the_help_and_schema_commands() {
        let output = run(Some("help"), &[]).expect("help succeeds");
        assert!(output.contains("  help"), "{output}");
        assert!(output.contains("  schema"), "{output}");
        assert!(output.contains("schema/snapshot.v0.json"), "{output}");
        assert!(output.contains("schema/transcript.v0.json"), "{output}");
    }

    #[test]
    fn unknown_command_is_an_error_naming_it() {
        let message = run(Some("nope"), &[]).expect_err("an unknown command fails");
        assert!(message.contains("`nope`"), "{message}");
    }

    #[test]
    fn schema_writes_every_file_that_check_then_accepts() {
        let directory = TemporaryDirectory::new("write");
        let output = schema(&[], directory.path()).expect("schema writes the files");
        for file in &SCHEMAS {
            assert!(output.contains(file.path), "{output}");
            let written =
                std::fs::read_to_string(directory.join(file.path)).expect("the file exists");
            assert_eq!(written, generated_schema(file).expect("schema serialises"));
            assert!(written.ends_with("}\n"), "trailing newline");
            assert!(
                written.contains("\"$schema\": \"https://json-schema.org/draft/2020-12/schema\""),
                "{} is draft 2020-12",
                file.path
            );
        }
        let output =
            schema(&["--check"], directory.path()).expect("check passes on matching content");
        assert_eq!(
            output.matches("up to date").count(),
            SCHEMAS.len(),
            "{output}"
        );
    }

    #[test]
    fn the_transcript_schema_has_the_transcript_fields() {
        let transcript = SCHEMAS
            .iter()
            .find(|file| file.path == "schema/transcript.v0.json")
            .expect("the transcript schema is listed");
        let value = (transcript.generate)();
        let required = value["required"].as_array().expect("required fields");
        for field in ["header", "nodes", "indexes", "excluded", "tab_order"] {
            assert!(required.iter().any(|name| name == field), "{field}");
        }
    }

    #[test]
    fn schema_check_fails_on_either_differing_file_and_writes_nothing() {
        for stale_file in &SCHEMAS {
            let directory = TemporaryDirectory::new("differ");
            schema(&[], directory.path()).expect("schema writes the files");
            let path = directory.join(stale_file.path);
            let stale = generated_schema(stale_file).expect("schema serialises") + " ";
            std::fs::write(&path, &stale).expect("stale file is written");
            let message = schema(&["--check"], directory.path())
                .expect_err("check fails on differing content");
            assert!(message.contains(stale_file.path), "{message}");
            assert!(message.contains("`cargo xtask schema`"), "{message}");
            for other in SCHEMAS.iter().filter(|other| other.path != stale_file.path) {
                assert!(!message.contains(other.path), "{message}");
            }
            assert_eq!(std::fs::read_to_string(&path).expect("file remains"), stale);
        }
    }

    #[test]
    fn schema_check_fails_when_the_files_are_missing() {
        let directory = TemporaryDirectory::new("missing");
        let message =
            schema(&["--check"], directory.path()).expect_err("check fails on missing files");
        for file in &SCHEMAS {
            assert!(message.contains(file.path), "{message}");
            assert!(!directory.join(file.path).exists(), "check writes nothing");
        }
        assert!(message.contains("`cargo xtask schema`"), "{message}");
    }

    #[test]
    fn schema_unknown_flag_is_an_error_naming_it() {
        let directory = TemporaryDirectory::new("flag");
        let message = schema(&["--nope"], directory.path()).expect_err("an unknown flag fails");
        assert!(message.contains("`--nope`"), "{message}");
        let message = schema(&["--check", "--nope"], directory.path())
            .expect_err("a trailing unknown flag fails");
        assert!(message.contains("`--nope`"), "{message}");
        for file in &SCHEMAS {
            assert!(
                !directory.join(file.path).exists(),
                "a rejected flag writes nothing"
            );
        }
    }
}
