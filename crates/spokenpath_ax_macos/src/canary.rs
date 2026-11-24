//! The check page capture loads before the real page, and what its tree says about the read.
//!
//! macOS answers a process without the Accessibility permission with an empty tree in some cases
//! rather than an error, so a read of a page whose expected nodes are known tells an empty tree
//! from a missing permission.

use std::collections::BTreeSet;

use spokenpath_snapshot::{SafariAxNode, SafariAxValue};

use crate::failure::{Code, Failure, PermissionSignal, permission_missing};

/// How many elements of the check page carry a check token, `sp-canary-1` to `sp-canary-K`.
pub(crate) const CANARY_TOKENS: usize = 3;

/// The check page, the single source of its HTML: each of its `CANARY_TOKENS` elements carries one
/// check token in its `class`.
pub(crate) const CANARY_PAGE: &str = r#"<!doctype html>
<html lang="en">
<meta charset="utf-8">
<title>spokenpath accessibility check</title>
<h1 class="sp-canary-1">spokenpath accessibility check</h1>
<p class="sp-canary-2">spokenpath reads Safari's macOS accessibility tree of this page before the capture.</p>
<button type="button" class="sp-canary-3">Check</button>
"#;

/// The attribute whose values carry the check tokens.
const CLASS_LIST: &str = "AXDOMClassList";

/// How many of the check tokens appear in an `AXDOMClassList` of the tree.
pub(crate) fn tokens_found(nodes: &[SafariAxNode]) -> usize {
    let expected: BTreeSet<String> = (1..=CANARY_TOKENS)
        .map(|index| format!("sp-canary-{index}"))
        .collect();
    let found: BTreeSet<&str> = nodes
        .iter()
        .filter_map(|node| match node.attributes.get(CLASS_LIST) {
            Some(SafariAxValue::Array { items }) => Some(items),
            _ => None,
        })
        .flatten()
        .filter_map(|item| match item {
            SafariAxValue::String { value } if expected.contains(value) => Some(value.as_str()),
            _ => None,
        })
        .collect();
    found.len()
}

/// The outcome of a check-page read that found `found` of the tokens, once the process is known
/// to be trusted and the screen unlocked: all of them is a usable read, none means the permission
/// is missing, and some is a tree that cannot be relied on.
pub(crate) fn classify(found: usize) -> Result<(), Failure> {
    match found {
        CANARY_TOKENS => Ok(()),
        0 => Err(permission_missing(PermissionSignal::EmptyCanary)),
        _ => Err(Failure::new(
            Code::AxCanaryIncomplete,
            format!(
                "Safari's macOS accessibility tree for the check page has {found} of its {CANARY_TOKENS} expected nodes while the page is visible and the screen is unlocked, so the tree cannot be relied on; run the capture again, and if it repeats, report it with the Safari and macOS versions."
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use spokenpath_snapshot::{SafariAxNode, SafariAxValue};

    use super::{CANARY_PAGE, CANARY_TOKENS, classify, tokens_found};
    use crate::failure::{Code, PERMISSION_HOW_TO};

    fn node(id: u32, classes: &[&str]) -> SafariAxNode {
        let items = classes
            .iter()
            .map(|class| SafariAxValue::String {
                value: (*class).to_owned(),
            })
            .collect();
        SafariAxNode {
            id,
            parent: None,
            children: Vec::new(),
            span: None,
            attributes: BTreeMap::from([(
                "AXDOMClassList".to_owned(),
                SafariAxValue::Array { items },
            )]),
        }
    }

    #[test]
    fn the_page_carries_each_token_once() {
        for index in 1..=CANARY_TOKENS {
            assert_eq!(
                CANARY_PAGE
                    .matches(&format!("\"sp-canary-{index}\""))
                    .count(),
                1,
                "sp-canary-{index}"
            );
        }
        assert!(!CANARY_PAGE.contains(&format!("sp-canary-{}", CANARY_TOKENS + 1)));
    }

    #[test]
    fn every_token_found_is_a_usable_read() {
        let nodes = [
            node(0, &[]),
            node(1, &["sp-canary-1"]),
            node(2, &["sp-canary-2", "other"]),
            node(3, &["sp-canary-3"]),
        ];
        assert_eq!(tokens_found(&nodes), 3);
        assert_eq!(classify(3), Ok(()));
    }

    #[test]
    fn no_token_found_is_a_missing_permission_with_reason_c() {
        assert_eq!(tokens_found(&[node(0, &[])]), 0);
        let failure = classify(0).expect_err("an empty check page fails");
        assert_eq!(failure.code, Code::AccessibilityPermissionMissing);
        assert_eq!(failure.code.exit_status(), 3);
        assert_eq!(
            failure.message,
            format!(
                "Safari's macOS accessibility tree for the check page has none of its 3 expected nodes while the page is visible and the screen is unlocked; macOS returns an empty tree when the process lacks the Accessibility permission. {PERMISSION_HOW_TO}"
            )
        );
    }

    #[test]
    fn some_tokens_found_is_an_incomplete_check_stating_both_counts() {
        let nodes = [node(1, &["sp-canary-1"]), node(2, &["sp-canary-1"])];
        assert_eq!(tokens_found(&nodes), 1);
        let failure = classify(1).expect_err("a partial check page fails");
        assert_eq!(failure.code, Code::AxCanaryIncomplete);
        assert_eq!(failure.code.exit_status(), 1);
        assert!(
            failure.message.starts_with(
                "Safari's macOS accessibility tree for the check page has 1 of its 3 expected nodes"
            ),
            "{}",
            failure.message
        );
    }

    #[test]
    fn a_token_outside_the_class_list_is_not_counted() {
        let mut title = node(1, &[]);
        title.attributes.insert(
            "AXTitle".to_owned(),
            SafariAxValue::String {
                value: "sp-canary-1".to_owned(),
            },
        );
        assert_eq!(tokens_found(&[title, node(2, &["sp-canary-4"])]), 0);
    }
}
