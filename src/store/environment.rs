use std::fmt;

use bevy::prelude::Resource;

/// The environment reported by the verified StoreKit 2 app transaction.
///
/// TestFlight uses [`Sandbox`](Self::Sandbox). Xcode without a StoreKit
/// configuration can also report sandbox, so this is deliberately not an
/// install-source detector. Wait for a terminal value, then select production
/// service configuration only when [`is_production`](Self::is_production)
/// returns `true`.
/// [`StorePlugin`](crate::store::StorePlugin) inserts this resource only on iOS.
///
/// <https://developer.apple.com/documentation/storekit/apptransaction/environment>
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AppStoreEnvironment {
    /// `AppTransaction.shared` is still resolving.
    #[default]
    Pending,
    /// StoreKit testing configured by Xcode.
    Xcode,
    /// The App Store sandbox, including TestFlight.
    Sandbox,
    /// The production App Store.
    Production,
    /// The app transaction could not be loaded or verified.
    Unavailable,
    /// StoreKit returned an environment this toolkit does not yet recognize.
    Unknown,
}

impl AppStoreEnvironment {
    /// Whether StoreKit has produced a terminal result.
    pub const fn is_resolved(self) -> bool {
        !matches!(self, Self::Pending)
    }

    /// Whether production-only service configuration is safe to use.
    ///
    /// Pending, test, unavailable, and unknown environments all return `false`
    /// so callers fail closed to their test configuration.
    pub const fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }
}

impl fmt::Display for AppStoreEnvironment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pending => "pending",
            Self::Xcode => "xcode",
            Self::Sandbox => "sandbox",
            Self::Production => "production",
            Self::Unavailable => "unavailable",
            Self::Unknown => "unknown",
        })
    }
}

pub(crate) const fn environment_from_raw(value: i32) -> AppStoreEnvironment {
    match value {
        0 => AppStoreEnvironment::Pending,
        1 => AppStoreEnvironment::Xcode,
        2 => AppStoreEnvironment::Sandbox,
        3 => AppStoreEnvironment::Production,
        4 => AppStoreEnvironment::Unavailable,
        _ => AppStoreEnvironment::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_values_are_explicit_and_unknown_values_fail_closed() {
        assert_eq!(environment_from_raw(0), AppStoreEnvironment::Pending);
        assert_eq!(environment_from_raw(1), AppStoreEnvironment::Xcode);
        assert_eq!(environment_from_raw(2), AppStoreEnvironment::Sandbox);
        assert_eq!(environment_from_raw(3), AppStoreEnvironment::Production);
        assert_eq!(environment_from_raw(4), AppStoreEnvironment::Unavailable);
        assert_eq!(environment_from_raw(5), AppStoreEnvironment::Unknown);
        assert_eq!(environment_from_raw(i32::MAX), AppStoreEnvironment::Unknown);
    }

    #[test]
    fn only_production_enables_production_configuration() {
        assert!(AppStoreEnvironment::Production.is_production());

        for environment in [
            AppStoreEnvironment::Pending,
            AppStoreEnvironment::Xcode,
            AppStoreEnvironment::Sandbox,
            AppStoreEnvironment::Unavailable,
            AppStoreEnvironment::Unknown,
        ] {
            assert!(!environment.is_production(), "{environment}");
        }
    }

    #[test]
    fn pending_is_the_only_unresolved_state() {
        assert!(!AppStoreEnvironment::Pending.is_resolved());

        for environment in [
            AppStoreEnvironment::Xcode,
            AppStoreEnvironment::Sandbox,
            AppStoreEnvironment::Production,
            AppStoreEnvironment::Unavailable,
            AppStoreEnvironment::Unknown,
        ] {
            assert!(environment.is_resolved(), "{environment}");
        }
    }
}
