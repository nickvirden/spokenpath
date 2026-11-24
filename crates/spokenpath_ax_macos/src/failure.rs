//! Why a command failed, the exit status each failure has, and the classifications of the
//! permission, lock and check-page signals, which are plain logic and run on every platform.

use serde_json::json;

use crate::canary::CANARY_TOKENS;

/// The how-to clause every missing-permission message ends with.
pub(crate) const PERMISSION_HOW_TO: &str = "Grant the Accessibility permission to the app that runs spokenpath, such as Terminal, in System Settings > Privacy & Security > Accessibility, then run the capture again; see \"Granting the Accessibility permission\" in docs/usage.md.";

/// Why a command failed. Each code is printed as its kebab-case name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Code {
    /// The command line is not usable.
    Usage,
    /// The helper runs on a platform without Safari's macOS accessibility tree.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    UnsupportedPlatform,
    /// The process lacks the Accessibility permission.
    AccessibilityPermissionMissing,
    /// The Mac's screen is locked.
    ScreenLocked,
    /// No single Safari process runs in the user's session.
    SafariNotRunning,
    /// No single Safari window and web area shows the exact URL.
    SafariWindowNotIdentified,
    /// The web area's URL changed while the tree was read.
    PageLeftOrigin,
    /// The read did not finish within `--timeout-ms`.
    Timeout,
    /// The tree has more nodes than the helper records.
    AxTreeTooLarge,
    /// The check page's tree has some but not all of its expected nodes.
    AxCanaryIncomplete,
    /// An accessibility call the read cannot do without failed.
    AxReadFailed,
}

impl Code {
    /// The code's name in the JSON error and on stderr.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Code::Usage => "usage",
            Code::UnsupportedPlatform => "unsupported-platform",
            Code::AccessibilityPermissionMissing => "accessibility-permission-missing",
            Code::ScreenLocked => "screen-locked",
            Code::SafariNotRunning => "safari-not-running",
            Code::SafariWindowNotIdentified => "safari-window-not-identified",
            Code::PageLeftOrigin => "page-left-origin",
            Code::Timeout => "timeout",
            Code::AxTreeTooLarge => "ax-tree-too-large",
            Code::AxCanaryIncomplete => "ax-canary-incomplete",
            Code::AxReadFailed => "ax-read-failed",
        }
    }

    /// The exit status: 3 for a missing permission, 2 for a usage error, 1 for anything else.
    pub(crate) fn exit_status(self) -> u8 {
        match self {
            Code::AccessibilityPermissionMissing => 3,
            Code::Usage => 2,
            _ => 1,
        }
    }
}

/// A failed command: its code and the message printed with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Failure {
    pub(crate) code: Code,
    pub(crate) message: String,
}

impl Failure {
    pub(crate) fn new(code: Code, message: impl Into<String>) -> Self {
        Failure {
            code,
            message: message.into(),
        }
    }

    /// The one JSON document printed on stdout.
    pub(crate) fn json(&self) -> String {
        json!({ "error": { "code": self.code.as_str(), "message": self.message } }).to_string()
    }

    /// The one line printed on stderr, without its line break.
    pub(crate) fn stderr_line(&self) -> String {
        format!(
            "spokenpath-ax-macos: {}: {}",
            self.code.as_str(),
            self.message
        )
    }
}

/// The failure off macOS.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) fn unsupported_platform() -> Failure {
    Failure::new(
        Code::UnsupportedPlatform,
        "Safari's macOS accessibility tree can be read only on macOS",
    )
}

/// Which signal says the permission is missing; each gives the message its own reason clause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PermissionSignal {
    /// `AXIsProcessTrusted()` returned false.
    Untrusted,
    /// An application-level call returned `kAXErrorAPIDisabled`.
    ApiDisabled,
    /// The check page's tree has none of its expected nodes.
    EmptyCanary,
}

/// `kAXErrorAPIDisabled`, which macOS returns to a process without the Accessibility permission.
pub(crate) const AX_ERROR_API_DISABLED: i32 = -25211;

/// The missing-permission failure, naming only the signal that fired.
pub(crate) fn permission_missing(signal: PermissionSignal) -> Failure {
    let reason = match signal {
        PermissionSignal::Untrusted => "macOS reports that this process is not trusted for Accessibility (AXIsProcessTrusted() is false).".to_owned(),
        PermissionSignal::ApiDisabled => "macOS refused the accessibility request with kAXErrorAPIDisabled (-25211), which it returns when the process lacks the Accessibility permission.".to_owned(),
        PermissionSignal::EmptyCanary => format!("Safari's macOS accessibility tree for the check page has none of its {CANARY_TOKENS} expected nodes while the page is visible and the screen is unlocked; macOS returns an empty tree when the process lacks the Accessibility permission."),
    };
    Failure::new(
        Code::AccessibilityPermissionMissing,
        format!("{reason} {PERMISSION_HOW_TO}"),
    )
}

/// Fails when `AXIsProcessTrusted()` returned false.
pub(crate) fn check_trusted(trusted: bool) -> Result<(), Failure> {
    if trusted {
        Ok(())
    } else {
        Err(permission_missing(PermissionSignal::Untrusted))
    }
}

/// Fails when an application-level call returned `kAXErrorAPIDisabled`.
pub(crate) fn check_api_enabled(error: i32) -> Result<(), Failure> {
    if error == AX_ERROR_API_DISABLED {
        Err(permission_missing(PermissionSignal::ApiDisabled))
    } else {
        Ok(())
    }
}

/// Fails when the session reports its screen locked. `locked` is the session dictionary's
/// `CGSSessionScreenIsLocked` value: absent means unlocked, as macOS sets the key only while the
/// screen is locked.
pub(crate) fn check_unlocked(locked: Option<bool>) -> Result<(), Failure> {
    if locked == Some(true) {
        Err(Failure::new(
            Code::ScreenLocked,
            "the Mac's screen is locked, so Safari's macOS accessibility tree cannot be read; keep the Mac awake and unlocked while Safari capture runs (see \"Enabling Safari automation\" in docs/usage.md)",
        ))
    } else {
        Ok(())
    }
}

/// Fails unless exactly one Safari process runs in the user's session, and returns its pid.
pub(crate) fn single_safari(pids: &[i32]) -> Result<i32, Failure> {
    match pids {
        [pid] => Ok(*pid),
        [] => Err(Failure::new(
            Code::SafariNotRunning,
            "Safari is not running in this user's session, so there is no automation window to read",
        )),
        several => Err(Failure::new(
            Code::SafariNotRunning,
            format!(
                "{} Safari processes run in this user's session, so the one showing the automation window cannot be told apart",
                several.len()
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AX_ERROR_API_DISABLED, Code, Failure, PERMISSION_HOW_TO, PermissionSignal,
        check_api_enabled, check_trusted, check_unlocked, permission_missing, single_safari,
        unsupported_platform,
    };

    #[test]
    fn an_untrusted_process_is_missing_the_permission_with_reason_a() {
        let failure = check_trusted(false).expect_err("untrusted fails");
        assert_eq!(failure.code, Code::AccessibilityPermissionMissing);
        assert_eq!(failure.code.exit_status(), 3);
        assert_eq!(
            failure.message,
            format!(
                "macOS reports that this process is not trusted for Accessibility (AXIsProcessTrusted() is false). {PERMISSION_HOW_TO}"
            )
        );
        assert_eq!(check_trusted(true), Ok(()));
    }

    #[test]
    fn api_disabled_is_missing_the_permission_with_reason_b_and_other_errors_are_not() {
        let failure = check_api_enabled(AX_ERROR_API_DISABLED).expect_err("-25211 fails");
        assert_eq!(failure.code, Code::AccessibilityPermissionMissing);
        assert!(
            failure.message.starts_with(
                "macOS refused the accessibility request with kAXErrorAPIDisabled (-25211),"
            ),
            "{}",
            failure.message
        );
        assert!(failure.message.ends_with(PERMISSION_HOW_TO));
        for error in [0, -25204, -25212] {
            assert_eq!(check_api_enabled(error), Ok(()), "{error}");
        }
    }

    #[test]
    fn each_permission_signal_names_only_its_own_cause() {
        let untrusted = permission_missing(PermissionSignal::Untrusted).message;
        let disabled = permission_missing(PermissionSignal::ApiDisabled).message;
        let empty = permission_missing(PermissionSignal::EmptyCanary).message;
        assert!(!untrusted.contains("-25211") && !untrusted.contains("check page"));
        assert!(!disabled.contains("AXIsProcessTrusted") && !disabled.contains("check page"));
        assert!(!empty.contains("AXIsProcessTrusted") && !empty.contains("-25211"));
        assert!(empty.starts_with(
            "Safari's macOS accessibility tree for the check page has none of its 3 expected nodes"
        ));
    }

    #[test]
    fn a_locked_screen_fails_and_an_absent_or_false_key_does_not() {
        let failure = check_unlocked(Some(true)).expect_err("locked fails");
        assert_eq!(failure.code, Code::ScreenLocked);
        assert_eq!(failure.code.exit_status(), 1);
        assert!(failure.message.starts_with("the Mac's screen is locked"));
        assert_eq!(check_unlocked(Some(false)), Ok(()));
        assert_eq!(check_unlocked(None), Ok(()));
    }

    #[test]
    fn exactly_one_safari_process_is_read() {
        assert_eq!(single_safari(&[42]), Ok(42));
        for pids in [&[][..], &[1, 2]] {
            let failure = single_safari(pids).expect_err("zero or two fail");
            assert_eq!(failure.code, Code::SafariNotRunning);
        }
    }

    #[test]
    fn a_failure_prints_one_json_document_and_one_stderr_line() {
        let failure = Failure::new(Code::Timeout, "the read did not finish \"in time\"");
        let parsed: serde_json::Value = serde_json::from_str(&failure.json()).expect("JSON");
        assert_eq!(
            parsed,
            serde_json::json!({
                "error": { "code": "timeout", "message": "the read did not finish \"in time\"" }
            })
        );
        assert!(!failure.json().contains('\n'));
        assert_eq!(
            failure.stderr_line(),
            "spokenpath-ax-macos: timeout: the read did not finish \"in time\""
        );
    }

    #[test]
    fn usage_errors_exit_2_and_other_failures_exit_1() {
        assert_eq!(Code::Usage.exit_status(), 2);
        for code in [
            Code::UnsupportedPlatform,
            Code::ScreenLocked,
            Code::SafariNotRunning,
            Code::SafariWindowNotIdentified,
            Code::PageLeftOrigin,
            Code::Timeout,
            Code::AxTreeTooLarge,
            Code::AxCanaryIncomplete,
            Code::AxReadFailed,
        ] {
            assert_eq!(code.exit_status(), 1, "{}", code.as_str());
        }
        assert_eq!(
            unsupported_platform().message,
            "Safari's macOS accessibility tree can be read only on macOS"
        );
    }
}
