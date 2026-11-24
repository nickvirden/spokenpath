//! The command line, parsed with `std` alone.

use std::ffi::OsString;
use std::time::Duration;

use crate::identify::WindowRect;

/// The usage line printed after every usage error.
pub(crate) const USAGE: &str = "usage: spokenpath-ax-macos read --url <url> --title <title> --window-rect <x>,<y>,<width>,<height> --timeout-ms <n> [--canary] | spokenpath-ax-macos canary-page | spokenpath-ax-macos --version";

/// The only origin the helper reads a page from: the loopback address capture serves pages on.
const LOOPBACK_PREFIX: &str = "http://127.0.0.1:";

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// Print the version line.
    Version,
    /// Print the check page's HTML.
    CanaryPage,
    /// Read Safari's macOS accessibility tree for one page.
    Read(ReadArgs),
}

/// The arguments of `read`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ReadArgs {
    /// The exact URL the automation window shows, nonce query included.
    pub(crate) url: String,
    /// The page's title, as `WebDriver` Get Title reports it, which the window's `AXTitle` equals.
    pub(crate) title: String,
    /// The automation window's rect, as `WebDriver` Get Window Rect reports it.
    pub(crate) window: WindowRect,
    /// How long the whole read may take.
    pub(crate) timeout: Duration,
    /// Whether the page is the check page, whose tokens are counted instead of recording the tree.
    pub(crate) canary: bool,
}

/// The command `args` describe, without the program name, or the usage error they make. Every
/// argument is echoed lossily, because the message goes to a terminal as text.
pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Err("no command given".to_owned());
    };
    let command = match command.to_str() {
        Some("--version") => Command::Version,
        Some("canary-page") => Command::CanaryPage,
        Some("read") => return read(args).map(Command::Read),
        _ => return Err(format!("unknown command `{}`", command.to_string_lossy())),
    };
    match args.next() {
        None => Ok(command),
        Some(extra) => Err(format!(
            "the command takes no arguments, but `{}` was given",
            extra.to_string_lossy()
        )),
    }
}

fn read(mut args: impl Iterator<Item = OsString>) -> Result<ReadArgs, String> {
    let mut url: Option<String> = None;
    let mut title: Option<String> = None;
    let mut window: Option<WindowRect> = None;
    let mut timeout: Option<Duration> = None;
    let mut canary = false;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--url") => {
                let value = value_of("--url", args.next())?;
                let Some(value) = value.to_str().filter(|value| is_loopback_url(value)) else {
                    return Err(format!(
                        "`--url` must be an http URL on 127.0.0.1 with a port, not `{}`",
                        value.to_string_lossy()
                    ));
                };
                if url.replace(value.to_owned()).is_some() {
                    return Err(repeated("--url"));
                }
            }
            Some("--title") => {
                let value = value_of("--title", args.next())?;
                let Some(value) = value.to_str() else {
                    return Err(format!(
                        "`--title` must be UTF-8, not `{}`",
                        value.to_string_lossy()
                    ));
                };
                if title.replace(value.to_owned()).is_some() {
                    return Err(repeated("--title"));
                }
            }
            Some("--window-rect") => {
                let value = value_of("--window-rect", args.next())?;
                let Some(rect) = value.to_str().and_then(WindowRect::parse) else {
                    return Err(format!(
                        "`--window-rect` must be four whole numbers `<x>,<y>,<width>,<height>`, not `{}`",
                        value.to_string_lossy()
                    ));
                };
                if window.replace(rect).is_some() {
                    return Err(repeated("--window-rect"));
                }
            }
            Some("--timeout-ms") => {
                let value = value_of("--timeout-ms", args.next())?;
                let Some(milliseconds) = value
                    .to_str()
                    .and_then(|value| value.parse::<u64>().ok())
                    .filter(|&milliseconds| milliseconds > 0)
                else {
                    return Err(format!(
                        "`--timeout-ms` must be a positive whole number of milliseconds, not `{}`",
                        value.to_string_lossy()
                    ));
                };
                if timeout
                    .replace(Duration::from_millis(milliseconds))
                    .is_some()
                {
                    return Err(repeated("--timeout-ms"));
                }
            }
            Some("--canary") => {
                if canary {
                    return Err(repeated("--canary"));
                }
                canary = true;
            }
            _ => return Err(format!("unknown argument `{}`", arg.to_string_lossy())),
        }
    }
    let Some(url) = url else {
        return Err("`read` needs `--url`".to_owned());
    };
    let Some(title) = title else {
        return Err("`read` needs `--title`".to_owned());
    };
    let Some(window) = window else {
        return Err("`read` needs `--window-rect`".to_owned());
    };
    let Some(timeout) = timeout else {
        return Err("`read` needs `--timeout-ms`".to_owned());
    };
    Ok(ReadArgs {
        url,
        title,
        window,
        timeout,
        canary,
    })
}

/// Whether `url` is `http://127.0.0.1:<port>` followed by a path, so the helper can only ever
/// match a page served on the loopback address.
fn is_loopback_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix(LOOPBACK_PREFIX) else {
        return false;
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && rest[digits..].starts_with('/')
}

fn value_of(flag: &str, value: Option<OsString>) -> Result<OsString, String> {
    value.ok_or_else(|| format!("`{flag}` needs a value"))
}

fn repeated(flag: &str) -> String {
    format!("`{flag}` is given more than once")
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::time::Duration;

    use super::{Command, ReadArgs, parse};
    use crate::identify::WindowRect;

    fn parsed(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(OsString::from))
    }

    /// `read` with the given URL and every other required value.
    fn read_with_url(url: &str) -> Result<Command, String> {
        parsed(&[
            "read",
            "--url",
            url,
            "--title",
            "t",
            "--window-rect",
            "0,0,1,1",
            "--timeout-ms",
            "1",
        ])
    }

    #[test]
    fn read_takes_the_url_the_timeout_and_the_canary_flag() {
        assert_eq!(
            parsed(&[
                "read",
                "--url",
                "http://127.0.0.1:5220/page.html?spokenpath-nonce=0",
                "--title",
                "Sign in",
                "--window-rect",
                "-1440,25,1280,800",
                "--timeout-ms",
                "5000",
                "--canary",
            ]),
            Ok(Command::Read(ReadArgs {
                url: "http://127.0.0.1:5220/page.html?spokenpath-nonce=0".to_owned(),
                title: "Sign in".to_owned(),
                window: WindowRect {
                    x: -1440,
                    y: 25,
                    width: 1280,
                    height: 800
                },
                timeout: Duration::from_millis(5000),
                canary: true,
            }))
        );
    }

    #[test]
    fn version_and_canary_page_take_no_arguments() {
        assert_eq!(parsed(&["--version"]), Ok(Command::Version));
        assert_eq!(parsed(&["canary-page"]), Ok(Command::CanaryPage));
        assert!(parsed(&["--version", "x"]).is_err());
        assert!(parsed(&["canary-page", "x"]).is_err());
    }

    #[test]
    fn read_refuses_a_url_off_the_loopback_address() {
        for url in [
            "https://127.0.0.1:1/x",
            "http://localhost:1/x",
            "http://127.0.0.1/x",
            "http://127.0.0.1:1",
            "http://127.0.0.1:1@example.com/x",
            "file:///x",
        ] {
            let error = read_with_url(url).expect_err("a non-loopback URL is refused");
            assert!(
                error.starts_with("`--url` must be an http URL on 127.0.0.1"),
                "{error}"
            );
        }
    }

    #[test]
    fn read_needs_every_value_once_each() {
        let all = [
            "read",
            "--url",
            "http://127.0.0.1:1/x",
            "--title",
            "t",
            "--window-rect",
            "0,0,1,1",
            "--timeout-ms",
            "1",
        ];
        assert!(parsed(&all).is_ok());
        for missing in [1, 3, 5, 7] {
            let mut args = all.to_vec();
            args.drain(missing..missing + 2);
            assert!(parsed(&args).is_err(), "{args:?}");
        }
        for (index, bad) in [
            (8, "0"),
            (8, "-1"),
            (6, "0,0,1"),
            (6, "0,0,1,x"),
            (6, "0,0,1,1,1"),
        ] {
            let mut args = all.to_vec();
            args[index] = bad;
            assert!(parsed(&args).is_err(), "{args:?}");
        }
        for repeated in [
            &["--timeout-ms", "1"][..],
            &["--title", "u"],
            &["--canary", "--canary"],
        ] {
            let mut args = all.to_vec();
            args.extend_from_slice(repeated);
            assert!(parsed(&args).is_err(), "{args:?}");
        }
        for args in [
            &["read", "--url"][..],
            &["read", "--frame", "x"],
            &["write"],
            &[],
        ] {
            assert!(parsed(args).is_err(), "{args:?}");
        }
    }
}
