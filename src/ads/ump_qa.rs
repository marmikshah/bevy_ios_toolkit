//! UMP QA launch switches read from environment variables.
//!
//! `<PREFIX>UMP_QA` selects a regional consent test: `eea` (also `1` or
//! `true`), `us` for a regulated US state, or `other`. A valid value applies
//! that [`UmpDebugGeography`] and resets consent at startup. `0` or `false`
//! turns QA off; an empty value counts as unset. Any other value is logged and
//! ignored. `<PREFIX>UMP_TEST_DEVICE_ID` adds the hashed device id UMP logs, for
//! QA on a physical device (simulators are test devices already). It is
//! ignored unless QA is active.
//!
//! Only development builds ([`AppEnvironment::is_development`]) read the
//! variables. Release device builds, including TestFlight, read nothing. The
//! ads plugin separately ignores [`UmpTestConfig`] unless
//! [`AdmobConfig::use_test_ads`] is true.

use std::ffi::OsString;

use super::{AdmobConfig, UmpDebugGeography, UmpTestConfig};
use crate::environment::AppEnvironment;

/// UMP QA settings for one launch: the [`UmpTestConfig`] resource to insert and
/// the test device ids for [`AdmobConfig::test_device_ids`].
///
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_ios_toolkit::ads::{AdmobConfig, UmpQa};
///
/// let qa = UmpQa::from_env("MYGAME_"); // MYGAME_UMP_QA, MYGAME_UMP_TEST_DEVICE_ID
/// let mut ads = AdmobConfig::for_build();
/// qa.apply(&mut ads);
/// App::new().insert_resource(qa.test).insert_resource(ads);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UmpQa {
    /// Insert as a resource before the ads plugin initializes.
    pub test: UmpTestConfig,
    /// Hashed physical test devices, empty unless QA is active.
    pub test_device_ids: Vec<String>,
}

impl UmpQa {
    /// Read `<prefix>UMP_QA` and `<prefix>UMP_TEST_DEVICE_ID` from the process
    /// environment, in development builds only.
    pub fn from_env(prefix: &str) -> Self {
        Self::from_vars(AppEnvironment::current(), prefix, |name| {
            std::env::var_os(name)
        })
    }

    /// [`from_env`](Self::from_env) with the build environment and variable
    /// lookup supplied by the caller. A production environment never calls
    /// `var`.
    pub fn from_vars(
        environment: AppEnvironment,
        prefix: &str,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Self {
        if !environment.is_development() {
            return Self::default();
        }
        let qa_name = format!("{prefix}UMP_QA");
        let device_name = format!("{prefix}UMP_TEST_DEVICE_ID");
        let geography = var(&qa_name).and_then(|value| geography(&qa_name, &value));
        let device = var(&device_name);
        let test_device_ids = match (geography, device) {
            (_, None) => Vec::new(),
            (None, Some(_)) => {
                tracing::warn!("{device_name} is ignored unless {qa_name} selects a geography");
                Vec::new()
            }
            (Some(_), Some(value)) => match value.to_str().map(str::trim) {
                None => {
                    tracing::error!("{device_name} is not valid Unicode; ignoring it");
                    Vec::new()
                }
                Some("") => {
                    tracing::warn!("{device_name} is empty; no physical test device applied");
                    Vec::new()
                }
                Some(id) => vec![id.to_owned()],
            },
        };
        let Some(geography) = geography else {
            return Self::default();
        };
        tracing::warn!(
            "UMP QA active ({geography:?}): consent resets at startup; overrides apply only with test ads"
        );
        Self {
            test: UmpTestConfig {
                geography: Some(geography),
                reset_consent_on_start: true,
            },
            test_device_ids,
        }
    }

    /// Whether a QA geography was selected.
    pub fn is_active(&self) -> bool {
        self.test.geography.is_some()
    }

    /// Add [`test_device_ids`](Self::test_device_ids) to `config`, skipping
    /// ids it already lists.
    pub fn apply(&self, config: &mut AdmobConfig) {
        for id in &self.test_device_ids {
            if !config.test_device_ids.contains(id) {
                config.test_device_ids.push(id.clone());
            }
        }
    }
}

fn geography(name: &str, value: &OsString) -> Option<UmpDebugGeography> {
    let Some(value) = value.to_str() else {
        tracing::error!("{name} is not valid Unicode; no UMP override applied");
        return None;
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "eea" | "1" | "true" => Some(UmpDebugGeography::Eea),
        "us" => Some(UmpDebugGeography::RegulatedUsState),
        "other" => Some(UmpDebugGeography::Other),
        "" | "0" | "false" => None,
        _ => {
            tracing::error!(
                "{name}={value:?} is invalid (expected eea, us, or other); no UMP override applied"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const DEV: AppEnvironment = AppEnvironment::Development;

    fn read(environment: AppEnvironment, vars: &[(&str, &str)]) -> UmpQa {
        let vars: HashMap<String, OsString> = vars
            .iter()
            .map(|(name, value)| (format!("GAME_{name}"), OsString::from(value)))
            .collect();
        UmpQa::from_vars(environment, "GAME_", |name| vars.get(name).cloned())
    }

    fn active(geography: UmpDebugGeography, ids: &[&str]) -> UmpQa {
        UmpQa {
            test: UmpTestConfig {
                geography: Some(geography),
                reset_consent_on_start: true,
            },
            test_device_ids: ids.iter().map(|id| id.to_string()).collect(),
        }
    }

    #[test]
    fn every_geography_resets_consent_and_applies_its_override() {
        for (value, geography) in [
            ("eea", UmpDebugGeography::Eea),
            ("1", UmpDebugGeography::Eea),
            ("true", UmpDebugGeography::Eea),
            ("EEA", UmpDebugGeography::Eea),
            ("us", UmpDebugGeography::RegulatedUsState),
            ("other", UmpDebugGeography::Other),
            (" other ", UmpDebugGeography::Other),
        ] {
            let qa = read(DEV, &[("UMP_QA", value)]);
            assert_eq!(qa, active(geography, &[]), "{value}");
            assert!(qa.is_active());
        }
    }

    #[test]
    fn unset_off_empty_and_unknown_values_apply_nothing() {
        assert_eq!(read(DEV, &[]), UmpQa::default());
        for value in ["", "  ", "0", "false", "uk", "eea,us"] {
            let qa = read(DEV, &[("UMP_QA", value), ("UMP_TEST_DEVICE_ID", "HASHED")]);
            assert_eq!(qa, UmpQa::default(), "{value}");
            assert!(!qa.is_active());
        }
    }

    #[test]
    fn device_id_applies_only_with_an_active_geography() {
        assert_eq!(
            read(DEV, &[("UMP_TEST_DEVICE_ID", "HASHED")]),
            UmpQa::default()
        );
        assert_eq!(
            read(
                DEV,
                &[("UMP_QA", "us"), ("UMP_TEST_DEVICE_ID", "  HASHED  ")]
            ),
            active(UmpDebugGeography::RegulatedUsState, &["HASHED"])
        );
        for empty in ["", "   "] {
            assert_eq!(
                read(DEV, &[("UMP_QA", "eea"), ("UMP_TEST_DEVICE_ID", empty)]),
                active(UmpDebugGeography::Eea, &[])
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_unicode_values_are_ignored() {
        use std::os::unix::ffi::OsStringExt;
        let bad = || Some(OsString::from_vec(vec![0xff, 0xfe]));
        let qa = UmpQa::from_vars(DEV, "GAME_", |name| match name {
            "GAME_UMP_QA" => bad(),
            _ => None,
        });
        assert_eq!(qa, UmpQa::default());
        let qa = UmpQa::from_vars(DEV, "GAME_", |name| match name {
            "GAME_UMP_QA" => Some("eea".into()),
            _ => bad(),
        });
        assert_eq!(qa, active(UmpDebugGeography::Eea, &[]));
    }

    #[test]
    fn production_builds_read_nothing() {
        let qa = UmpQa::from_vars(AppEnvironment::Production, "GAME_", |name| {
            panic!("production read {name}")
        });
        assert_eq!(qa, UmpQa::default());
    }

    #[test]
    fn the_prefix_names_both_variables() {
        let qa = UmpQa::from_vars(DEV, "OTHER_", |name| match name {
            "OTHER_UMP_QA" => Some("other".into()),
            "OTHER_UMP_TEST_DEVICE_ID" => Some("ID".into()),
            _ => None,
        });
        assert_eq!(qa, active(UmpDebugGeography::Other, &["ID"]));
        assert_eq!(read(DEV, &[("OTHER_UMP_QA", "eea")]), UmpQa::default());
    }

    #[test]
    fn apply_adds_device_ids_once() {
        let qa = active(UmpDebugGeography::Eea, &["A", "B"]);
        let mut config = AdmobConfig::for_build();
        config.test_device_ids.push("A".into());
        qa.apply(&mut config);
        qa.apply(&mut config);
        assert_eq!(config.test_device_ids, ["A", "B"]);
        UmpQa::default().apply(&mut config);
        assert_eq!(config.test_device_ids, ["A", "B"]);
    }
}
