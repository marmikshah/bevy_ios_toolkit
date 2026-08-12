use std::collections::HashSet;

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

/// One purchasable product, including StoreKit's localized display price.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductInfo {
    pub id: String,
    pub display_name: String,
    /// Localized, currency-formatted price returned by `Product.displayPrice`.
    pub display_price: String,
    pub description: String,
}

/// Loading state of the product catalogue.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ProductsState {
    #[default]
    Loading,
    Ready,
    Failed,
}

/// Resolution state of the authoritative StoreKit entitlement snapshot.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntitlementsState {
    /// StoreKit has not completed its first `currentEntitlements` read.
    #[default]
    Checking,
    /// `owns` reflects a completed, verified StoreKit snapshot, including an
    /// authoritative empty result for an account with no purchases.
    Ready,
    /// The latest reconciliation could not establish verified ownership.
    Failed,
}

/// The fetched catalogue. Mirrors the backend; read-only to consumers.
#[derive(Resource, Default)]
pub struct StoreProducts {
    pub state: ProductsState,
    pub items: Vec<ProductInfo>,
}

impl StoreProducts {
    pub fn get(&self, id: &str) -> Option<&ProductInfo> {
        self.items.iter().find(|product| product.id == id)
    }
}

/// Runtime StoreKit ownership. Never persist or infer this resource in a game.
/// Wait for [`EntitlementsState::Ready`] before treating an absent id as
/// confirmed unowned. On a later failure, the last verified set is retained
/// while `state()` reports the failure. Consumables never appear here.
#[derive(Resource, Default)]
pub struct Entitlements {
    state: EntitlementsState,
    owned: HashSet<String>,
}

impl Entitlements {
    pub const fn state(&self) -> EntitlementsState {
        self.state
    }

    pub const fn is_ready(&self) -> bool {
        matches!(self.state, EntitlementsState::Ready)
    }

    pub fn owns(&self, id: &str) -> bool {
        self.owned.contains(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.owned.iter()
    }

    pub(crate) fn apply(&mut self, snapshot: EntitlementSnapshot) -> bool {
        let previous_state = self.state;
        let previous_owned = self.owned.clone();
        self.state = snapshot.state;
        if snapshot.state != EntitlementsState::Checking {
            self.owned = snapshot.product_ids.into_iter().collect();
        }
        self.state != previous_state || self.owned != previous_owned
    }

    pub(crate) fn fail(&mut self) -> bool {
        if self.state == EntitlementsState::Failed {
            return false;
        }
        self.state = EntitlementsState::Failed;
        true
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct EntitlementSnapshot {
    state: EntitlementsState,
    product_ids: Vec<String>,
}

pub(crate) fn decode_entitlement_snapshot(
    json: &str,
) -> Result<EntitlementSnapshot, serde_json::Error> {
    serde_json::from_str(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_empty_snapshot_is_authoritative() {
        let mut entitlements = Entitlements::default();

        let snapshot =
            decode_entitlement_snapshot(r#"{"state":"ready","product_ids":[]}"#).unwrap();

        assert!(entitlements.apply(snapshot));
        assert_eq!(entitlements.state(), EntitlementsState::Ready);
        assert!(entitlements.is_ready());
        assert!(!entitlements.owns("com.example.supporter"));
    }

    #[test]
    fn revocation_replaces_owned_snapshot_with_empty() {
        let mut entitlements = Entitlements::default();
        let owned = decode_entitlement_snapshot(
            r#"{"state":"ready","product_ids":["com.example.supporter"]}"#,
        )
        .unwrap();
        let revoked = decode_entitlement_snapshot(r#"{"state":"ready","product_ids":[]}"#).unwrap();

        assert!(entitlements.apply(owned));
        assert!(entitlements.owns("com.example.supporter"));
        assert!(entitlements.apply(revoked));
        assert!(!entitlements.owns("com.example.supporter"));
    }

    #[test]
    fn failed_refresh_retains_last_verified_ownership() {
        let mut entitlements = Entitlements::default();
        let owned = decode_entitlement_snapshot(
            r#"{"state":"ready","product_ids":["com.example.supporter"]}"#,
        )
        .unwrap();
        entitlements.apply(owned);

        let failed = decode_entitlement_snapshot(
            r#"{"state":"failed","product_ids":["com.example.supporter"]}"#,
        )
        .unwrap();

        assert!(entitlements.apply(failed));
        assert_eq!(entitlements.state(), EntitlementsState::Failed);
        assert!(entitlements.owns("com.example.supporter"));
    }

    #[test]
    fn malformed_snapshot_is_not_an_empty_entitlement() {
        assert!(decode_entitlement_snapshot("[]").is_err());
        assert!(
            decode_entitlement_snapshot(r#"{"state":"ready","product_ids":"not-an-array"}"#)
                .is_err()
        );
    }
}
