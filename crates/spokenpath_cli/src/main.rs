//! The `spokenpath-core` binary, the command-line entry point to the Rust core.
//!
//! `spokenpath-core transcript <snapshot | -> [--source <file>] [--cross-check <snapshot | ->]
//! [--format text|json] [--no-speech]` reads a snapshot from a file or from stdin (`-`), adapts
//! it, and writes its transcript: the text transcript by default, or with `--format json` the
//! serialised `spokenpath_announce::Transcript`. `--no-speech` leaves out Layer B. `--source` names
//! the page file the snapshot was captured from; it is matched to a snapshot source by SHA-256 and
//! gives the text output its excerpts. `--cross-check` names a Safari `WebDriver` snapshot of the
//! same page, read the same way and under the same limit, whose roles and names the transcript's
//! are compared with; at most one of the two snapshots may come from stdin. The binary opens no
//! other file: never a path named inside a snapshot. `spokenpath-core --version` prints the
//! version line.
//!
//! Exit codes: 0 on success; 2 for a usage error or bad input (an unreadable file, an input over
//! the 32 MiB limit, a snapshot the reader or the adapter rejects, a `--source` that matches no
//! source, or a cross-check snapshot that is not a Safari `WebDriver` snapshot of the same page); 3
//! for a snapshot whose major version the reader does not read; 1 when writing the output fails.
//! Every failure writes one `spokenpath-core: ` line to stderr, and the capture's diagnostics go to
//! stderr as `spokenpath-core: diagnostic <code>: <message>` lines, the cross-check snapshot's as
//! `spokenpath-core: diagnostic safari-webdriver <code>: <message>`.

use std::process::ExitCode;

mod args;
mod run;
mod text;

#[cfg(test)]
mod tests;

fn version_line() -> String {
    format!("spokenpath-core {}", env!("CARGO_PKG_VERSION"))
}

fn main() -> ExitCode {
    let code = run::run(
        std::env::args_os().skip(1),
        &mut std::io::stdin().lock(),
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
        env!("CARGO_PKG_VERSION"),
    );
    ExitCode::from(code)
}
