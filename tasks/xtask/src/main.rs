//! Repository automation, run as `cargo xtask <command>`.

use std::process::ExitCode;

/// Each command's name and the one-line description that `help` prints.
const COMMANDS: &[(&str, &str)] = &[("help", "List the available commands")];

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

/// Runs `command` and returns what to print, or an error message when the
/// command is unknown. No command at all prints the help.
fn run(command: Option<&str>) -> Result<String, String> {
    match command {
        None | Some("help") => Ok(help()),
        Some(unknown) => Err(format!(
            "unknown command `{unknown}`; run `cargo xtask help` to list the commands"
        )),
    }
}

fn main() -> ExitCode {
    let command = std::env::args().nth(1);
    match run(command.as_deref()) {
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
    use super::run;

    #[test]
    fn help_lists_the_help_command() {
        let output = run(Some("help")).expect("help succeeds");
        assert!(output.contains("  help"), "{output}");
    }

    #[test]
    fn unknown_command_is_an_error_naming_it() {
        let message = run(Some("nope")).expect_err("an unknown command fails");
        assert!(message.contains("`nope`"), "{message}");
    }
}
