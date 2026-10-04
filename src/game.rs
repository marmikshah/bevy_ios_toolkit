//! Default iOS game services: quiet StoreKit loading and serialized launch
//! permissions, independent of gameplay readiness. Games own placements and
//! reminder content; SDK policy stays in the toolkit.

use crate::{
    IosPlugin, IosSystems,
    ads::*,
    att::TrackingStatus,
    environment::AppEnvironment,
    notifications::{NotificationPermission, RequestNotificationPermission},
    store::StoreConfig,
};
use bevy::prelude::*;

#[derive(Clone, Copy, Default)]
enum Mode {
    #[default]
    Live,
    Capture,
    CaptureStore,
}

/// One product and ad configuration for the normal game composition.
#[derive(Clone)]
pub struct IosGameConfig {
    pub product_ids: Vec<String>,
    pub ads: AdmobConfig,
    mode: Mode,
}

impl IosGameConfig {
    pub fn new(product_ids: impl IntoIterator<Item = impl Into<String>>, ads: AdmobConfig) -> Self {
        Self {
            product_ids: product_ids.into_iter().map(Into::into).collect(),
            ads,
            mode: Mode::Live,
        }
    }

    /// Disable native requests for an explicit capture. Ignored on production
    /// device builds, including TestFlight, regardless of transaction environment.
    pub fn capture(mut self) -> Self {
        self.mode = Mode::Capture;
        self
    }

    /// Local StoreKit review capture: quiet store loading without ad/permission UI.
    /// Ignored on production devices.
    pub fn capture_store(mut self) -> Self {
        self.mode = Mode::CaptureStore;
        self
    }

    fn services(&self, environment: AppEnvironment) -> (bool, bool) {
        match (environment.is_development(), self.mode) {
            (true, Mode::Capture) => (false, false),
            (true, Mode::CaptureStore) => (true, false),
            _ => (true, true),
        }
    }

    fn ads_for(&self, environment: AppEnvironment) -> AdmobConfig {
        let mut ads = self.ads.clone();
        ads.tracking_prompt = AdTrackingPrompt::Automatic;
        ads.use_test_ads = environment.is_development();
        ads
    }
}

/// Installs the mandatory game profile. There is no all-services-ready gate:
/// an unresolved permission or failed SDK must not stop rendering or input.
pub struct IosGamePlugin(pub IosGameConfig);

impl Plugin for IosGamePlugin {
    fn build(&self, app: &mut App) {
        let environment = AppEnvironment::current();
        let (store, prompts) = self.0.services(environment);
        if !app.is_plugin_added::<IosPlugin>() {
            app.add_plugins(IosPlugin);
        }
        if !app.is_plugin_added::<BannerPlugin>() {
            app.add_plugins(BannerPlugin);
        }
        if store {
            app.insert_resource(StoreConfig {
                product_ids: self.0.product_ids.clone(),
            });
        }
        if prompts {
            app.insert_resource(self.0.ads_for(environment))
                .init_resource::<LaunchPermissions>()
                .add_systems(Update, launch_permissions.in_set(IosSystems::Intents));
        }
    }
}

#[derive(Resource, Default)]
struct LaunchPermissions {
    first_frame: bool,
    notification_requested: bool,
}

impl LaunchPermissions {
    fn should_request(
        &mut self,
        tracking: TrackingStatus,
        consent_complete: bool,
        notifications: NotificationPermission,
    ) -> bool {
        if !self.first_frame {
            self.first_frame = true;
            return false;
        }
        if self.notification_requested
            || !tracking.is_determined()
            || !consent_complete
            || notifications != NotificationPermission::NotDetermined
        {
            return false;
        }
        self.notification_requested = true;
        true
    }
}

fn launch_permissions(
    mut flow: ResMut<LaunchPermissions>,
    tracking: Res<TrackingStatus>,
    ads: Res<AdmobState>,
    notifications: Res<NotificationPermission>,
    mut requests: MessageWriter<RequestNotificationPermission>,
) {
    if flow.should_request(*tracking, ads.consent_check_complete, *notifications) {
        requests.write(RequestNotificationPermission);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_settings_read_and_unfinished_consent_do_not_prompt() {
        let mut flow = LaunchPermissions::default();
        assert!(!flow.should_request(
            TrackingStatus::Denied,
            true,
            NotificationPermission::NotDetermined
        ));
        assert!(!flow.should_request(
            TrackingStatus::Denied,
            true,
            NotificationPermission::Checking
        ));
        assert!(!flow.should_request(
            TrackingStatus::NotDetermined,
            true,
            NotificationPermission::NotDetermined
        ));
        assert!(!flow.should_request(
            TrackingStatus::Denied,
            false,
            NotificationPermission::NotDetermined
        ));
        assert!(flow.should_request(
            TrackingStatus::Denied,
            true,
            NotificationPermission::NotDetermined
        ));
        assert!(!flow.should_request(
            TrackingStatus::Denied,
            true,
            NotificationPermission::NotDetermined
        ));
    }

    #[test]
    fn settled_permission_is_not_requested_again() {
        for permission in [
            NotificationPermission::Denied,
            NotificationPermission::Authorized,
            NotificationPermission::Provisional,
        ] {
            let mut flow = LaunchPermissions::default();
            for _ in 0..5 {
                assert!(!flow.should_request(TrackingStatus::Authorized, true, permission));
            }
        }
    }

    #[cfg(not(target_os = "ios"))]
    #[test]
    fn sdk_failure_does_not_block_updates_or_the_next_permission() {
        let _tracking = crate::att::test_support::guard();
        let _notifications = crate::notifications::tests::guarded();
        crate::att::test_support::set_status(2);
        crate::ads::reset_test_backend();
        // SAFETY: shared fake guards serialize every test touching these knobs.
        unsafe {
            std::env::set_var("BEVY_ADMOB_FAKE_CONSENT_UPDATE", "failed");
            std::env::set_var("BEVY_ADMOB_FAKE_CAN_REQUEST_ADS", "0");
        }
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(IosGamePlugin(IosGameConfig::new(
                ["app.supporter"],
                AdmobConfig::for_build(),
            )));
        #[derive(Resource, Default)]
        struct Frames(u32);
        app.init_resource::<Frames>()
            .add_systems(Update, |mut frames: ResMut<Frames>| frames.0 += 1);
        for _ in 0..10 {
            app.update();
        }
        assert_eq!(app.world().resource::<Frames>().0, 10);
        assert!(!app.world().resource::<AdmobState>().can_request_ads);
        assert!(app.world().resource::<AdmobState>().consent_check_complete);
        assert_eq!(
            *app.world().resource::<NotificationPermission>(),
            NotificationPermission::Authorized
        );
        unsafe {
            std::env::remove_var("BEVY_ADMOB_FAKE_CONSENT_UPDATE");
            std::env::remove_var("BEVY_ADMOB_FAKE_CAN_REQUEST_ADS");
        }
    }

    #[test]
    fn production_ignores_capture_requests() {
        let config = IosGameConfig::new(["app.supporter"], AdmobConfig::for_build());
        assert_eq!(
            config
                .clone()
                .capture()
                .services(AppEnvironment::Production),
            (true, true)
        );
        assert_eq!(
            config
                .clone()
                .capture_store()
                .services(AppEnvironment::Production),
            (true, true)
        );
        assert_eq!(
            config
                .clone()
                .capture()
                .services(AppEnvironment::Development),
            (false, false)
        );
        assert_eq!(
            config.capture_store().services(AppEnvironment::Development),
            (true, false)
        );
    }

    #[test]
    fn game_profile_enforces_build_policy_even_when_a_caller_supplies_test_ads() {
        let config = IosGameConfig::new(["app.supporter"], AdmobConfig::test_ads());
        assert!(!config.ads_for(AppEnvironment::Production).use_test_ads);
        assert!(config.ads_for(AppEnvironment::Development).use_test_ads);
        assert_eq!(
            config.ads_for(AppEnvironment::Production).tracking_prompt,
            AdTrackingPrompt::Automatic
        );
    }
}
