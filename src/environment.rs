//! Application service selection follows the build, never a StoreKit receipt.
//! TestFlight purchases use Apple's sandbox, but the app runs production services.
//!
//! [`dev_var`] and [`dev_flag`] read a game's launch switches (capture modes,
//! QA overrides) in development builds only, so a release device build never
//! reads them.
//!
//! ```no_run
//! use bevy_ios_toolkit::environment::{dev_flag, dev_var};
//!
//! let skip_prompts = dev_flag("MYGAME_NO_PROMPT");
//! let window = dev_var("MYGAME_WINDOW"); // e.g. Some("390x844")
//! ```

use std::ffi::OsString;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppEnvironment {
    Development,
    Production,
}

impl AppEnvironment {
    pub const fn from_build(debug: bool, simulator: bool) -> Self {
        if debug || simulator {
            Self::Development
        } else {
            Self::Production
        }
    }

    pub const fn current() -> Self {
        Self::from_build(cfg!(debug_assertions), cfg!(target_abi = "sim"))
    }

    pub const fn is_development(self) -> bool {
        matches!(self, Self::Development)
    }
}

/// The environment variable `name` in a development build, as
/// [`AppEnvironment::current`] defines it (debug or simulator). `None` in
/// production, and when the variable is unset, empty or not valid Unicode. The
/// value is returned as set, untrimmed.
pub fn dev_var(name: &str) -> Option<String> {
    var_in(AppEnvironment::current(), name, |name| {
        std::env::var_os(name)
    })
}

/// Whether the switch `name` is on in a development build: its
/// [`dev_var`] value is `1` or `true` (any case), ignoring surrounding
/// whitespace. Always false in production.
pub fn dev_flag(name: &str) -> bool {
    dev_var(name).is_some_and(|value| is_on(&value))
}

/// Production never calls `var`.
fn var_in(
    environment: AppEnvironment,
    name: &str,
    var: impl Fn(&str) -> Option<OsString>,
) -> Option<String> {
    if !environment.is_development() {
        return None;
    }
    var(name)?
        .into_string()
        .ok()
        .filter(|value| !value.is_empty())
}

fn is_on(value: &str) -> bool {
    let value = value.trim();
    value == "1" || value.eq_ignore_ascii_case("true")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_variables_are_read_only_in_development() {
        let set = |name: &str| (name == "GAME_SCREEN").then(|| OsString::from("390x844"));
        assert_eq!(
            var_in(AppEnvironment::Development, "GAME_SCREEN", set),
            Some("390x844".to_string())
        );
        assert_eq!(var_in(AppEnvironment::Development, "GAME_OTHER", set), None);
        let never = |_: &str| -> Option<OsString> { panic!("production read a variable") };
        assert_eq!(
            var_in(AppEnvironment::Production, "GAME_SCREEN", never),
            None
        );
        let empty = |_: &str| Some(OsString::new());
        assert_eq!(
            var_in(AppEnvironment::Development, "GAME_SCREEN", empty),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_values_are_unset() {
        use std::os::unix::ffi::OsStringExt;
        let invalid = |_: &str| Some(OsString::from_vec(vec![0x66, 0xff]));
        assert_eq!(var_in(AppEnvironment::Development, "GAME", invalid), None);
    }

    #[test]
    fn flags_are_on_for_one_or_true() {
        for on in ["1", "true", "TRUE", " True ", "1\n"] {
            assert!(is_on(on), "{on:?}");
        }
        for off in ["0", "false", "yes", "", "11", "on"] {
            assert!(!is_on(off), "{off:?}");
        }
    }

    #[test]
    fn unset_switches_are_off() {
        assert_eq!(dev_var("BEVY_IOS_TOOLKIT_TEST_UNSET_SWITCH"), None);
        assert!(!dev_flag("BEVY_IOS_TOOLKIT_TEST_UNSET_SWITCH"));
    }

    #[test]
    fn only_debug_or_simulator_builds_use_development_services() {
        for (debug, simulator, expected) in [
            (false, false, AppEnvironment::Production),
            (false, true, AppEnvironment::Development),
            (true, false, AppEnvironment::Development),
            (true, true, AppEnvironment::Development),
        ] {
            assert_eq!(AppEnvironment::from_build(debug, simulator), expected);
        }
    }
}
