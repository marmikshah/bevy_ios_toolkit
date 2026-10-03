//! Application service selection follows the build, never a StoreKit receipt.
//! TestFlight purchases use Apple's sandbox, but the app runs production services.

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

#[cfg(test)]
mod tests {
    use super::*;

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
