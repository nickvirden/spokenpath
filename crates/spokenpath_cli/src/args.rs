//! The command line, parsed with `std` alone.

use std::ffi::OsString;
use std::path::PathBuf;

/// The usage line printed after every usage error.
pub(crate) const USAGE: &str = "usage: spokenpath-core transcript <snapshot | -> [--source <file>] [--format text|json] [--no-speech] | spokenpath-core --version";

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// Print the version line.
    Version,
    /// Print the transcript of a snapshot.
    Transcript(TranscriptArgs),
}

/// The arguments of `transcript`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TranscriptArgs {
    /// Where the snapshot comes from.
    pub(crate) snapshot: Input,
    /// The file whose bytes are the snapshot source with the same SHA-256.
    pub(crate) source: Option<PathBuf>,
    /// How the transcript is written.
    pub(crate) format: Format,
    /// Whether Layer B is left out.
    pub(crate) no_speech: bool,
}

/// Where the snapshot comes from.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Input {
    /// Standard input, named `-` on the command line.
    Stdin,
    /// A file.
    Path(PathBuf),
}

/// How the transcript is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    /// The text transcript for a terminal.
    Text,
    /// The serialised `Transcript`.
    Json,
}

/// The command `args` describe, without the program name, or the usage error they make. Every
/// argument is echoed lossily, because the message goes to a terminal as text.
pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Err("no command given".to_owned());
    };
    match command.to_str() {
        Some("--version") => match args.next() {
            None => Ok(Command::Version),
            Some(extra) => Err(format!(
                "`--version` takes no arguments, but `{}` was given",
                extra.to_string_lossy()
            )),
        },
        Some("transcript") => transcript(args).map(Command::Transcript),
        _ => Err(format!("unknown command `{}`", command.to_string_lossy())),
    }
}

fn transcript(mut args: impl Iterator<Item = OsString>) -> Result<TranscriptArgs, String> {
    let mut snapshot: Option<Input> = None;
    let mut source: Option<PathBuf> = None;
    let mut format: Option<Format> = None;
    let mut no_speech = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--source") => {
                let value = value_of("--source", args.next())?;
                if source.replace(PathBuf::from(value)).is_some() {
                    return Err(repeated("--source"));
                }
            }
            Some("--format") => {
                let value = value_of("--format", args.next())?;
                let parsed = match value.to_str() {
                    Some("text") => Format::Text,
                    Some("json") => Format::Json,
                    _ => {
                        return Err(format!(
                            "unknown format `{}`; the formats are `text` and `json`",
                            value.to_string_lossy()
                        ));
                    }
                };
                if format.replace(parsed).is_some() {
                    return Err(repeated("--format"));
                }
            }
            Some("--no-speech") => {
                if no_speech {
                    return Err(repeated("--no-speech"));
                }
                no_speech = true;
            }
            Some("-") => set_snapshot(&mut snapshot, Input::Stdin)?,
            _ if arg.to_string_lossy().starts_with('-') => {
                return Err(format!("unknown flag `{}`", arg.to_string_lossy()));
            }
            _ => set_snapshot(&mut snapshot, Input::Path(PathBuf::from(arg)))?,
        }
    }
    let Some(snapshot) = snapshot else {
        return Err("no snapshot given; name a file, or `-` to read it from stdin".to_owned());
    };
    Ok(TranscriptArgs {
        snapshot,
        source,
        format: format.unwrap_or(Format::Text),
        no_speech,
    })
}

fn value_of(flag: &str, value: Option<OsString>) -> Result<OsString, String> {
    value.ok_or_else(|| format!("`{flag}` needs a value"))
}

fn repeated(flag: &str) -> String {
    format!("`{flag}` is given more than once")
}

fn set_snapshot(snapshot: &mut Option<Input>, input: Input) -> Result<(), String> {
    if snapshot.replace(input).is_some() {
        return Err("more than one snapshot given".to_owned());
    }
    Ok(())
}
