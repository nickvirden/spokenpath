//! Repository automation, run as `cargo xtask <command>`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Each command's name and the one-line description that `help` prints.
const COMMANDS: &[(&str, &str)] = &[
    ("help", "List the available commands"),
    (
        "schema",
        "Write schema/snapshot.v0.json from the snapshot types; --check compares it instead",
    ),
];

/// The committed JSON Schema, relative to the repository root.
const SCHEMA_PATH: &str = "schema/snapshot.v0.json";

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
fn generated_schema() -> Result<String, String> {
    serde_json::to_string_pretty(&spokenpath_snapshot::json_schema())
        .map(|text| text + "\n")
        .map_err(|error| format!("could not serialise the snapshot schema: {error}"))
}

/// Writes `contents` to `path`, creating its directory.
fn write_schema(path: &Path, contents: &str) -> Result<String, String> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .map_err(|error| format!("could not create {}: {error}", directory.display()))?;
    }
    std::fs::write(path, contents)
        .map_err(|error| format!("could not write {SCHEMA_PATH}: {error}"))?;
    Ok(format!("wrote {SCHEMA_PATH}\n"))
}

/// Succeeds when the file at `path` is byte-identical to `contents`, and writes nothing.
fn check_schema(path: &Path, contents: &str) -> Result<String, String> {
    match std::fs::read(path) {
        Ok(committed) if committed == contents.as_bytes() => {
            Ok(format!("{SCHEMA_PATH} is up to date\n"))
        }
        Ok(_) => Err(format!(
            "{SCHEMA_PATH} differs from the schema the snapshot types generate; run `cargo xtask schema` and commit the result"
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "{SCHEMA_PATH} is missing; run `cargo xtask schema` and commit the result"
        )),
        Err(error) => Err(format!("could not read {SCHEMA_PATH}: {error}")),
    }
}

/// Runs `schema` against the file at `path`: writes it, or with `--check` compares it.
fn schema(flags: &[&str], path: &Path) -> Result<String, String> {
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
    let contents = generated_schema()?;
    if check {
        check_schema(path, &contents)
    } else {
        write_schema(path, &contents)
    }
}

/// Runs `command` with its `flags` and returns what to print, or an error message when the
/// command is unknown or fails. No command at all prints the help.
fn run(command: Option<&str>, flags: &[&str]) -> Result<String, String> {
    match command {
        None | Some("help") => Ok(help()),
        Some("schema") => schema(flags, &repository_root().join(SCHEMA_PATH)),
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
    use std::path::PathBuf;

    use super::{generated_schema, run, schema};

    /// A fresh directory under the system temporary directory, unique to this process and test.
    fn temporary_directory(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("xtask-{}-{name}", std::process::id()));
        if directory.exists() {
            std::fs::remove_dir_all(&directory).expect("stale test directory is removed");
        }
        std::fs::create_dir_all(&directory).expect("test directory is created");
        directory
    }

    #[test]
    fn help_lists_the_help_and_schema_commands() {
        let output = run(Some("help"), &[]).expect("help succeeds");
        assert!(output.contains("  help"), "{output}");
        assert!(output.contains("  schema"), "{output}");
    }

    #[test]
    fn unknown_command_is_an_error_naming_it() {
        let message = run(Some("nope"), &[]).expect_err("an unknown command fails");
        assert!(message.contains("`nope`"), "{message}");
    }

    #[test]
    fn schema_writes_a_file_that_check_then_accepts() {
        let path = temporary_directory("write").join("schema/snapshot.v0.json");
        schema(&[], &path).expect("schema writes the file");
        let written = std::fs::read_to_string(&path).expect("the file exists");
        assert_eq!(written, generated_schema().expect("schema serialises"));
        assert!(written.ends_with("}\n"), "trailing newline");
        let output = schema(&["--check"], &path).expect("check passes on matching content");
        assert!(output.contains("up to date"), "{output}");
    }

    #[test]
    fn schema_check_fails_on_differing_content_and_writes_nothing() {
        let path = temporary_directory("differ").join("snapshot.v0.json");
        let stale = generated_schema().expect("schema serialises") + " ";
        std::fs::write(&path, &stale).expect("stale file is written");
        let message = schema(&["--check"], &path).expect_err("check fails on differing content");
        assert!(message.contains("schema/snapshot.v0.json"), "{message}");
        assert!(message.contains("`cargo xtask schema`"), "{message}");
        assert_eq!(std::fs::read_to_string(&path).expect("file remains"), stale);
    }

    #[test]
    fn schema_check_fails_when_the_file_is_missing() {
        let path = temporary_directory("missing").join("snapshot.v0.json");
        let message = schema(&["--check"], &path).expect_err("check fails on a missing file");
        assert!(message.contains("schema/snapshot.v0.json"), "{message}");
        assert!(message.contains("`cargo xtask schema`"), "{message}");
        assert!(!path.exists(), "check writes nothing");
    }

    #[test]
    fn schema_unknown_flag_is_an_error_naming_it() {
        let path = temporary_directory("flag").join("snapshot.v0.json");
        let message = schema(&["--nope"], &path).expect_err("an unknown flag fails");
        assert!(message.contains("`--nope`"), "{message}");
        let message =
            schema(&["--check", "--nope"], &path).expect_err("a trailing unknown flag fails");
        assert!(message.contains("`--nope`"), "{message}");
        assert!(!path.exists(), "a rejected flag writes nothing");
    }
}
