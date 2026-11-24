//! The `read` command on macOS: the permission and lock checks, then window identification, the
//! walk, and the checks after it, over the safe wrappers in [`crate::ax`].

use std::time::{Duration, Instant};

use spokenpath_snapshot::SafariAxNode;

use crate::args::ReadArgs;
use crate::ax::{self, CfString, Element};
use crate::failure::{
    Code, Failure, check_api_enabled, check_trusted, check_unlocked, single_safari,
};
use crate::identify::{
    IDENTIFY_EVERY, IDENTIFY_FOR, NotIdentified, WindowRect, single_web_area, single_window,
};
use crate::tree::{CHILDREN, Raw, Source, walk};

/// The longest any one accessibility message to Safari may wait for an answer.
const MESSAGING_TIMEOUT_SECONDS: f32 = 5.0;

/// The most elements of the window's own interface searched for its web areas.
const MAX_WINDOW_ELEMENTS: usize = 10_000;

/// The role of a page's root accessibility element.
const WEB_AREA: &str = "AXWebArea";

/// The overall time limit of one read.
struct Deadline {
    end: Instant,
    limit: Duration,
}

impl Deadline {
    fn check(&self) -> Result<(), Failure> {
        if Instant::now() >= self.end {
            Err(Failure::new(
                Code::Timeout,
                format!(
                    "reading Safari's macOS accessibility tree did not finish within {} ms",
                    self.limit.as_millis()
                ),
            ))
        } else {
            Ok(())
        }
    }
}

/// The attribute names the helper reads by name, made once.
struct Names {
    windows: CfString,
    title: CfString,
    position: CfString,
    size: CfString,
    role: CfString,
    children: CfString,
    url: CfString,
}

impl Names {
    fn new() -> Result<Names, Failure> {
        let make = |name: &str| {
            CfString::new(name).ok_or_else(|| {
                Failure::new(
                    Code::AxReadFailed,
                    format!("the attribute name {name} cannot be made"),
                )
            })
        };
        Ok(Names {
            windows: make("AXWindows")?,
            title: make("AXTitle")?,
            position: make("AXPosition")?,
            size: make("AXSize")?,
            role: make("AXRole")?,
            children: make(CHILDREN)?,
            url: make("AXURL")?,
        })
    }
}

/// What the read found: the tree, or the check page's token count.
pub(crate) enum Read {
    Tree(Vec<SafariAxNode>),
    Canary { found: usize },
}

/// Reads Safari's macOS accessibility tree for the page at `args.url`.
///
/// # Errors
///
/// Every failure the command can report, as [`Failure`].
pub(crate) fn read(args: &ReadArgs) -> Result<Read, Failure> {
    let deadline = Deadline {
        end: Instant::now() + args.timeout,
        limit: args.timeout,
    };
    check_trusted(ax::process_trusted())?;
    check_unlocked(ax::screen_locked())?;
    let pids = ax::safari_pids().map_err(|reason| {
        Failure::new(
            Code::AxReadFailed,
            format!("Safari's process cannot be listed: {reason}"),
        )
    })?;
    let safari = Element::application(single_safari(&pids)?).ok_or_else(|| {
        Failure::new(
            Code::AxReadFailed,
            "Safari's application element cannot be made",
        )
    })?;
    safari.set_messaging_timeout(MESSAGING_TIMEOUT_SECONDS);
    let names = Names::new()?;
    let title = CfString::new(&args.title)
        .ok_or_else(|| Failure::new(Code::AxReadFailed, "the title cannot be made a CFString"))?;
    let window = Expected {
        url: &args.url,
        title: &title,
        rect: args.window,
    };
    let web_area = identify(&safari, &names, &window, &deadline)?;
    let mut source = MacSource {
        deadline: &deadline,
        names: Vec::new(),
    };
    let (nodes, _) = walk(&mut source, web_area.clone())?;
    if url_of(&web_area, &names).as_deref() != Some(args.url.as_str()) {
        return Err(Failure::new(
            Code::PageLeftOrigin,
            "the web area's URL changed while Safari's macOS accessibility tree was read, as it does when the page navigates away, so the tree is not the served page's; capture detects such a navigation and does not prevent it",
        ));
    }
    check_unlocked(ax::screen_locked())?;
    Ok(if args.canary {
        Read::Canary {
            found: crate::canary::tokens_found(&nodes),
        }
    } else {
        Read::Tree(nodes)
    })
}

/// What `WebDriver` reports about the automation window and its page.
struct Expected<'a> {
    url: &'a str,
    title: &'a CfString,
    rect: WindowRect,
}

/// Repeats identification while it may still succeed, for up to [`IDENTIFY_FOR`].
fn identify(
    safari: &Element,
    names: &Names,
    expected: &Expected<'_>,
    deadline: &Deadline,
) -> Result<Element, Failure> {
    let give_up = Instant::now() + IDENTIFY_FOR;
    loop {
        deadline.check()?;
        match identify_once(safari, names, expected, deadline)? {
            Ok(web_area) => return Ok(web_area),
            Err(not_identified) if !not_identified.may_resolve() || Instant::now() >= give_up => {
                return Err(not_identified.failure());
            }
            Err(_) => std::thread::sleep(IDENTIFY_EVERY),
        }
    }
}

/// One identification attempt. From Safari's window list it reads only each window's `AXTitle`,
/// compared with the page's title without being copied, and for a window with that title its
/// `AXPosition` and `AXSize`; it descends only into the one window that matches.
fn identify_once(
    safari: &Element,
    names: &Names,
    expected: &Expected<'_>,
    deadline: &Deadline,
) -> Result<Result<Element, NotIdentified>, Failure> {
    let windows = match safari.attribute(&names.windows) {
        Ok(windows) => windows,
        Err(error) => {
            check_api_enabled(error)?;
            return Ok(Err(NotIdentified::NoWindow));
        }
    };
    let windows: Vec<Element> = match windows.raw() {
        Raw::Array(items) => items
            .into_iter()
            .filter_map(|item| match item {
                Raw::Element(window) => Some(window),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let window = match single_window(&windows, |window| {
        window.set_messaging_timeout(MESSAGING_TIMEOUT_SECONDS);
        window
            .attribute(&names.title)
            .is_ok_and(|title| title.is_string(expected.title))
            && matches!(
                (point_of(window, &names.position), point_of(window, &names.size)),
                (Some(position), Some(size)) if expected.rect.matches(position, size)
            )
    }) {
        Ok(window) => window,
        Err(not_identified) => return Ok(Err(not_identified)),
    };
    let web_areas = web_areas_in(window, names, deadline)?;
    let urls: Vec<Option<String>> = web_areas
        .iter()
        .map(|web_area| url_of(web_area, names))
        .collect();
    Ok(single_web_area(expected.url, &urls).map(|index| web_areas[index].clone()))
}

/// A point or size attribute as two numbers, or `None` when it cannot be read as one.
fn point_of(element: &Element, name: &CfString) -> Option<(f64, f64)> {
    match element.attribute(name).ok()?.raw() {
        Raw::Point { x, y } => Some((x, y)),
        Raw::Size { width, height } => Some((width, height)),
        _ => None,
    }
}

/// The web areas in the window's own interface, found breadth first without descending into any
/// web area.
fn web_areas_in(
    window: &Element,
    names: &Names,
    deadline: &Deadline,
) -> Result<Vec<Element>, Failure> {
    let mut queue = std::collections::VecDeque::from([window.clone()]);
    let mut searched = 0;
    let mut found = Vec::new();
    while let Some(element) = queue.pop_front() {
        deadline.check()?;
        searched += 1;
        if searched > MAX_WINDOW_ELEMENTS {
            break;
        }
        element.set_messaging_timeout(MESSAGING_TIMEOUT_SECONDS);
        let role = element.attribute(&names.role).ok().map(|role| role.raw());
        if matches!(&role, Some(Raw::String(units)) if String::from_utf16_lossy(units) == WEB_AREA)
        {
            found.push(element);
            continue;
        }
        if let Ok(children) = element.attribute(&names.children)
            && let Raw::Array(items) = children.raw()
        {
            queue.extend(items.into_iter().filter_map(|item| match item {
                Raw::Element(child) => Some(child),
                _ => None,
            }));
        }
    }
    Ok(found)
}

/// The web area's `AXURL` as text, or `None` when it cannot be read.
fn url_of(web_area: &Element, names: &Names) -> Option<String> {
    match web_area.attribute(&names.url).ok()?.raw() {
        Raw::Url(units) | Raw::String(units) => Some(String::from_utf16_lossy(&units)),
        _ => None,
    }
}

/// The walk's view of the accessibility API.
struct MacSource<'a> {
    deadline: &'a Deadline,
    /// The `CFString` of each attribute name the last `attribute_names` call returned.
    names: Vec<(String, CfString)>,
}

impl Source for MacSource<'_> {
    type Element = Element;

    fn attribute_names(&mut self, element: &Element) -> Result<Vec<String>, i32> {
        element.set_messaging_timeout(MESSAGING_TIMEOUT_SECONDS);
        self.names = element.attribute_names()?;
        Ok(self.names.iter().map(|(name, _)| name.clone()).collect())
    }

    fn attribute(&mut self, element: &Element, name: &str) -> Raw<Element> {
        let Some((_, cf_name)) = self.names.iter().find(|(candidate, _)| candidate == name) else {
            return Raw::Error(-25205);
        };
        match element.attribute(cf_name) {
            Ok(value) => value.raw(),
            Err(error) => Raw::Error(error),
        }
    }

    fn same(&self, left: &Element, right: &Element) -> bool {
        left.same(right)
    }

    fn hash(&self, element: &Element) -> u64 {
        element.hash()
    }

    fn check_deadline(&self) -> Result<(), Failure> {
        self.deadline.check()
    }
}
