//! Which Safari window and web area the helper reads.
//!
//! Safari does not report a window's `AXDocument`, so the window is picked by its top-level
//! `AXTitle`, `AXPosition` and `AXSize`: the single window whose title is the page's title and
//! whose rect is the automation window's, both as `WebDriver` reports them. Inside it, the single
//! `AXWebArea` whose `AXURL` is the exact URL, nonce included, is read. The rule is plain logic
//! over what was read, so it runs on every platform; no message names anything read from a window.

use std::time::Duration;

use crate::failure::{Code, Failure};

/// How long identification is repeated while no matching window or web area exists yet.
pub(crate) const IDENTIFY_FOR: Duration = Duration::from_secs(5);

/// How long the helper waits between two identification attempts.
pub(crate) const IDENTIFY_EVERY: Duration = Duration::from_millis(250);

/// A window's position and size in screen points, as `WebDriver` Get Window Rect reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowRect {
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) width: i64,
    pub(crate) height: i64,
}

impl WindowRect {
    /// `<x>,<y>,<width>,<height>`, four whole numbers; the width and height are positive.
    pub(crate) fn parse(text: &str) -> Option<WindowRect> {
        let numbers: Vec<i64> = text
            .split(',')
            .map(|part| part.parse().ok())
            .collect::<Option<_>>()?;
        match numbers[..] {
            [x, y, width, height] if width > 0 && height > 0 => Some(WindowRect {
                x,
                y,
                width,
                height,
            }),
            _ => None,
        }
    }

    /// Whether an `AXPosition` and `AXSize` describe this rect, each within half a point, as
    /// `WebDriver` reports whole points.
    pub(crate) fn matches(self, position: (f64, f64), size: (f64, f64)) -> bool {
        let same = |accessibility: f64, webdriver: i64| {
            // Screen coordinates are far inside the range an f64 holds exactly.
            #[allow(clippy::cast_precision_loss)]
            let webdriver = webdriver as f64;
            (accessibility - webdriver).abs() <= 0.5
        };
        same(position.0, self.x)
            && same(position.1, self.y)
            && same(size.0, self.width)
            && same(size.1, self.height)
    }
}

/// Why one identification attempt found no single window and web area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotIdentified {
    /// No window has the page's title and the automation window's rect.
    NoWindow,
    /// More than one window has them.
    SeveralWindows(usize),
    /// The window has no web area.
    NoWebArea,
    /// The window's web areas have other URLs.
    WebAreaUrlDiffers,
    /// More than one web area in the window has the URL.
    SeveralWebAreas(usize),
}

impl NotIdentified {
    /// Whether a later attempt can succeed, as when the page has not finished loading yet. Two
    /// matches stay two, so that failure is final at once.
    pub(crate) fn may_resolve(self) -> bool {
        !matches!(
            self,
            NotIdentified::SeveralWindows(_) | NotIdentified::SeveralWebAreas(_)
        )
    }

    /// The failure, which states counts only and quotes nothing read from any window.
    pub(crate) fn failure(self) -> Failure {
        let detail = match self {
            NotIdentified::NoWindow => {
                "no Safari window has the page's title and the automation window's position and size".to_owned()
            }
            NotIdentified::SeveralWindows(count) => format!(
                "{count} Safari windows have the page's title and the automation window's position and size"
            ),
            NotIdentified::NoWebArea => "the matching Safari window has no web area".to_owned(),
            NotIdentified::WebAreaUrlDiffers => {
                "no web area in the matching Safari window is at exactly the capture URL".to_owned()
            }
            NotIdentified::SeveralWebAreas(count) => format!(
                "{count} web areas in the matching Safari window are at the capture URL"
            ),
        };
        Failure::new(
            Code::SafariWindowNotIdentified,
            format!(
                "the automation window cannot be identified, so nothing was read: {detail}; capture reads only the one window showing the page it served"
            ),
        )
    }
}

/// The single window `is_automation_window` accepts.
pub(crate) fn single_window<W>(
    windows: &[W],
    is_automation_window: impl Fn(&W) -> bool,
) -> Result<&W, NotIdentified> {
    let mut matching = windows.iter().filter(|window| is_automation_window(window));
    match (matching.next(), matching.count()) {
        (None, _) => Err(NotIdentified::NoWindow),
        (Some(window), 0) => Ok(window),
        (Some(_), more) => Err(NotIdentified::SeveralWindows(more + 1)),
    }
}

/// The index of the single web area whose URL equals `url` character for character, among the
/// web areas of the identified window; `None` is a web area whose URL cannot be read.
pub(crate) fn single_web_area(
    url: &str,
    web_area_urls: &[Option<String>],
) -> Result<usize, NotIdentified> {
    let mut matching = web_area_urls
        .iter()
        .enumerate()
        .filter(|(_, candidate)| candidate.as_deref() == Some(url));
    match (matching.next(), matching.count()) {
        (None, _) if web_area_urls.is_empty() => Err(NotIdentified::NoWebArea),
        (None, _) => Err(NotIdentified::WebAreaUrlDiffers),
        (Some((index, _)), 0) => Ok(index),
        (Some(_), more) => Err(NotIdentified::SeveralWebAreas(more + 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::{NotIdentified, WindowRect, single_web_area, single_window};
    use crate::failure::Code;

    const URL: &str =
        "http://127.0.0.1:5220/sign-in.html?spokenpath-nonce=0123456789abcdef0123456789abcdef";

    const RECT: WindowRect = WindowRect {
        x: 0,
        y: 25,
        width: 1280,
        height: 812,
    };

    /// A window's top-level attributes as identification sees them.
    struct Window {
        title: &'static str,
        position: (f64, f64),
        size: (f64, f64),
    }

    const AUTOMATION: Window = Window {
        title: "Sign in",
        position: (0.0, 25.0),
        size: (1280.0, 812.0),
    };

    fn pick(windows: &[Window]) -> Result<&Window, NotIdentified> {
        single_window(windows, |window| {
            window.title == "Sign in" && RECT.matches(window.position, window.size)
        })
    }

    fn assert_quotes_nothing(failure: NotIdentified, read: &[&str]) {
        let failure = failure.failure();
        assert_eq!(failure.code, Code::SafariWindowNotIdentified);
        for text in read {
            assert!(
                !failure.message.contains(text),
                "{} quotes {text}",
                failure.message
            );
        }
        assert!(!failure.message.contains("http"), "{}", failure.message);
    }

    #[test]
    fn zero_candidate_windows_fail_and_may_resolve() {
        let windows = [
            Window {
                title: "Bank statement",
                ..AUTOMATION
            },
            Window {
                position: (10.0, 25.0),
                ..AUTOMATION
            },
            Window {
                size: (1280.0, 900.0),
                ..AUTOMATION
            },
        ];
        let failure = pick(&windows)
            .map(|window| window.title)
            .expect_err("none match");
        assert_eq!(failure, NotIdentified::NoWindow);
        assert!(failure.may_resolve());
        assert_quotes_nothing(failure, &["Bank statement", "Sign in"]);
    }

    #[test]
    fn two_candidate_windows_fail_at_once() {
        let windows = [AUTOMATION, AUTOMATION];
        let failure = pick(&windows)
            .map(|window| window.title)
            .expect_err("two match");
        assert_eq!(failure, NotIdentified::SeveralWindows(2));
        assert!(!failure.may_resolve());
        assert_quotes_nothing(failure, &["Sign in", URL]);
    }

    #[test]
    fn one_exact_match_is_identified() {
        let windows = [
            Window {
                title: "Other",
                ..AUTOMATION
            },
            Window {
                position: (0.4, 24.6),
                size: (1280.2, 811.5),
                ..AUTOMATION
            },
        ];
        assert_eq!(
            pick(&windows).map(|window| window.size),
            Ok((1280.2, 811.5))
        );
        assert_eq!(single_web_area(URL, &[None, Some(URL.to_owned())]), Ok(1));
    }

    #[test]
    fn a_web_area_whose_url_differs_only_in_its_nonce_fails() {
        let stale = URL.replace(
            "0123456789abcdef0123456789abcdef",
            "fedcba9876543210fedcba9876543210",
        );
        let failure = single_web_area(URL, &[Some(stale.clone())]).expect_err("the nonce differs");
        assert_eq!(failure, NotIdentified::WebAreaUrlDiffers);
        assert!(failure.may_resolve());
        assert_quotes_nothing(failure, &[&stale, "fedcba"]);
    }

    #[test]
    fn a_web_area_url_must_match_character_for_character() {
        for near in [
            format!("{URL}#top"),
            URL.replace("127.0.0.1:5220", "127.0.0.1:5221"),
            URL.replace("sign-in", "Sign-in"),
            URL.to_uppercase(),
            URL.trim_end_matches('f').to_owned(),
        ] {
            assert_eq!(
                single_web_area(URL, &[Some(near.clone())]),
                Err(NotIdentified::WebAreaUrlDiffers),
                "{near}"
            );
        }
    }

    #[test]
    fn a_window_without_web_areas_or_with_two_at_the_url_fails() {
        assert_eq!(single_web_area(URL, &[]), Err(NotIdentified::NoWebArea));
        let failure = single_web_area(URL, &[Some(URL.to_owned()), Some(URL.to_owned())])
            .expect_err("two web areas match");
        assert_eq!(failure, NotIdentified::SeveralWebAreas(2));
        assert!(!failure.may_resolve());
        assert_quotes_nothing(failure, &[URL]);
    }

    #[test]
    fn a_window_rect_is_four_whole_numbers_with_a_positive_size() {
        assert_eq!(WindowRect::parse("0,25,1280,812"), Some(RECT));
        assert_eq!(
            WindowRect::parse("-1440,-30,800,600").map(|rect| (rect.x, rect.y)),
            Some((-1440, -30))
        );
        for bad in [
            "",
            "0,25,1280",
            "0,25,1280,812,1",
            "0,25,0,812",
            "0,25,1280.5,812",
            "a,b,c,d",
        ] {
            assert_eq!(WindowRect::parse(bad), None, "{bad}");
        }
    }
}
