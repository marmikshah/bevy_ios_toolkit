use bevy::prelude::Resource;

/// The one StoreKit operation currently in flight.
///
/// Consumers render progress from this resource and suppress duplicate actions
/// while it is not [`Idle`](Self::Idle). Terminal details arrive through
/// `PurchaseCompleted` or `RestoreCompleted`.
#[derive(Resource, Clone, Debug, PartialEq, Eq, Default)]
pub enum StoreActivity {
    #[default]
    Idle,
    Purchasing {
        product_id: String,
    },
    Restoring,
}

impl StoreActivity {
    pub const fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }
}

pub(crate) fn start_purchase(activity: &mut StoreActivity, product_id: &str) -> bool {
    if !activity.is_idle() {
        return false;
    }
    *activity = StoreActivity::Purchasing {
        product_id: product_id.to_string(),
    };
    true
}

pub(crate) fn start_restore(activity: &mut StoreActivity) -> bool {
    if !activity.is_idle() {
        return false;
    }
    *activity = StoreActivity::Restoring;
    true
}

pub(crate) fn finish_activity(activity: &mut StoreActivity) {
    *activity = StoreActivity::Idle;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purchase_activity_names_the_product_and_suppresses_overlap() {
        let mut activity = StoreActivity::Idle;

        assert!(start_purchase(&mut activity, "com.example.supporter"));
        assert_eq!(
            activity,
            StoreActivity::Purchasing {
                product_id: "com.example.supporter".to_string(),
            }
        );
        assert!(!start_purchase(&mut activity, "com.example.second"));
        assert!(!start_restore(&mut activity));

        finish_activity(&mut activity);
        assert!(activity.is_idle());
    }

    #[test]
    fn restore_activity_suppresses_overlap_until_completion() {
        let mut activity = StoreActivity::Idle;

        assert!(start_restore(&mut activity));
        assert_eq!(activity, StoreActivity::Restoring);
        assert!(!start_restore(&mut activity));
        assert!(!start_purchase(&mut activity, "com.example.supporter"));

        finish_activity(&mut activity);
        assert!(activity.is_idle());
    }
}
