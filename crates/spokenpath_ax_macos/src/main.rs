//! The `spokenpath-ax-macos` binary, which reads Safari's macOS accessibility tree for the page
//! that Safari capture shows in its automation window, and records it raw.
//!
//! `spokenpath-ax-macos read --url <url> --title <title> --window-rect <x>,<y>,<width>,<height>
//! --timeout-ms <n> [--canary]` checks that the process holds the Accessibility permission and
//! that the screen is unlocked, finds the single running Safari, the single Safari window whose
//! `AXTitle` is `<title>` and whose `AXPosition` and `AXSize` are the rect (the page's title and
//! the automation window's rect, as `WebDriver` reports them) and, in it, the single `AXWebArea`
//! whose `AXURL` is exactly `<url>`, and walks that web area's tree. It prints
//! `{"nodes":[...]}`, the nodes of a `safari-ax` snapshot payload, or with `--canary`
//! `{"canary":{"expected":K,"found":K}}` for the check page. `<url>` must be an http URL on
//! 127.0.0.1. `spokenpath-ax-macos canary-page` prints `{"html":"..."}`, the check page.
//! `spokenpath-ax-macos --version` prints the version line.
//!
//! On failure it prints `{"error":{"code":"<code>","message":"<message>"}}` on stdout and
//! `spokenpath-ax-macos: <code>: <message>` on stderr. Exit statuses: 0 on success, 2 for a usage
//! error, 3 when the Accessibility permission is missing, and 1 for any other failure.
//!
//! The helper reads only through Safari's application element: never the system-wide element or
//! another process. Of each window it reads only `AXTitle`, and `AXPosition` and `AXSize` of a
//! window whose title matches. Inside the identified window it reads `AXRole` and `AXChildren` of
//! the window's own interface and `AXURL` of each web area there, and outputs only the tree of the
//! single web area at the exact URL. It never sets an attribute or prompts for the permission.
//! Off macOS, `read` fails with `unsupported-platform`.

use std::io::Write;
use std::process::ExitCode;

use serde_json::json;

mod args;
// Every `unsafe` call of the workspace lives in this one module, which allows it for itself.
#[cfg(target_os = "macos")]
mod ax;
// The check, classification, identification and walk logic runs only on macOS, where `read` is
// supported, but is tested on every platform.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod canary;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod failure;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod identify;
#[cfg(target_os = "macos")]
mod read;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod tree;

use args::{Command, ReadArgs, USAGE};
use failure::{Code, Failure};

/// What a successful command prints on stdout.
fn run(command: &Command) -> Result<String, Failure> {
    match command {
        Command::Version => Ok(format!("spokenpath-ax-macos {}", env!("CARGO_PKG_VERSION"))),
        Command::CanaryPage => Ok(json!({ "html": canary::CANARY_PAGE }).to_string()),
        Command::Read(args) => read_tree(args),
    }
}

#[cfg(target_os = "macos")]
fn read_tree(args: &ReadArgs) -> Result<String, Failure> {
    match read::read(args)? {
        read::Read::Tree(nodes) => Ok(json!({ "nodes": nodes }).to_string()),
        read::Read::Canary { found } => {
            canary::classify(found)?;
            Ok(
                json!({ "canary": { "expected": canary::CANARY_TOKENS, "found": found } })
                    .to_string(),
            )
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn read_tree(_args: &ReadArgs) -> Result<String, Failure> {
    Err(failure::unsupported_platform())
}

fn main() -> ExitCode {
    let result = args::parse(std::env::args_os().skip(1))
        .map_err(|message| Failure::new(Code::Usage, format!("{message}; {USAGE}")))
        .and_then(|command| run(&command));
    let (stdout, status) = match &result {
        Ok(output) => (output.clone(), 0),
        Err(failure) => {
            // stderr is only for people; a write to it that fails changes nothing.
            let _ = writeln!(std::io::stderr().lock(), "{}", failure.stderr_line());
            (failure.json(), failure.code.exit_status())
        }
    };
    let mut out = std::io::stdout().lock();
    if writeln!(out, "{stdout}")
        .and_then(|()| out.flush())
        .is_err()
    {
        return ExitCode::from(1);
    }
    ExitCode::from(status)
}

#[cfg(test)]
mod tests {
    use super::{Command, run};
    use crate::canary::CANARY_PAGE;

    #[test]
    fn canary_page_prints_the_page_as_one_json_document() {
        let output = run(&Command::CanaryPage).expect("canary-page succeeds");
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("JSON");
        assert_eq!(parsed, serde_json::json!({ "html": CANARY_PAGE }));
        assert!(!output.contains('\n'));
    }

    #[test]
    fn version_prints_the_version_line() {
        assert_eq!(
            run(&Command::Version),
            Ok(format!("spokenpath-ax-macos {}", env!("CARGO_PKG_VERSION")))
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn read_off_macos_fails_with_unsupported_platform_and_exit_1() {
        let failure = run(&Command::Read(crate::args::ReadArgs {
            url: "http://127.0.0.1:1/x".to_owned(),
            title: String::new(),
            window: crate::identify::WindowRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            timeout: std::time::Duration::from_secs(5),
            canary: false,
        }))
        .expect_err("read is unsupported off macOS");
        assert_eq!(failure.code, crate::failure::Code::UnsupportedPlatform);
        assert_eq!(failure.code.exit_status(), 1);
        assert_eq!(
            failure.message,
            "Safari's macOS accessibility tree can be read only on macOS"
        );
    }
}
