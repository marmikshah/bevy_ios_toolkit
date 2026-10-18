use super::*;

/// One-shot helper systems keyed off a `Local` latch.
fn load_interstitial_once(mut loads: MessageWriter<LoadAd>, mut fired: Local<bool>) {
    if !*fired {
        *fired = true;
        loads.write(LoadAd(AdFormat::Interstitial));
    }
}

fn show_when_loaded(
    inventory: Res<AdInventory>,
    mut shows: MessageWriter<ShowAd>,
    mut fired: Local<bool>,
) {
    if !*fired && inventory.is_loaded(AdFormat::Interstitial) {
        *fired = true;
        shows.write(ShowAd(AdFormat::Interstitial));
    }
}

fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(AdsPlugin);
    app
}

/// The fake backend is a process-global singleton, so tests that drive it
/// must not run concurrently. Holding this guard serializes them and resets
/// the fake (and clears env knobs) to a clean slate.
fn guarded() -> std::sync::MutexGuard<'static, ()> {
    let g = crate::att::test_support::guard();
    // Existing ad-flow tests begin with an already answered ATT decision.
    crate::att::test_support::set_status(3);
    backend::reset();
    for key in [
        "BEVY_ADMOB_FAKE_NO_FILL",
        "BEVY_ADMOB_FAKE_SHOW_FAIL",
        "BEVY_ADMOB_FAKE_REWARD_AMOUNT",
        "BEVY_ADMOB_FAKE_REWARD_TYPE",
        "BEVY_ADMOB_FAKE_CONSENT",
        "BEVY_ADMOB_FAKE_CAN_REQUEST_ADS",
        "BEVY_ADMOB_FAKE_CONSENT_UPDATE",
        "BEVY_ADMOB_FAKE_CONSENT_PENDING",
        "BEVY_ADMOB_FAKE_PRIVACY_OPTIONS",
    ] {
        unsafe { std::env::remove_var(key) };
    }
    g
}

#[test]
fn format_i32_round_trips() {
    for f in AdFormat::ALL {
        assert_eq!(AdFormat::from_i32(f.as_i32()), Some(f));
    }
    assert_eq!(AdFormat::from_i32(99), None);
}

#[test]
fn resolve_unit_prefers_config_then_falls_back_to_test() {
    let cfg = AdmobConfig::default().with_unit(AdFormat::Interstitial, "ca-app-pub-x/y");
    assert_eq!(cfg.resolve_unit(AdFormat::Interstitial), "ca-app-pub-x/y");
    // Unconfigured format falls back to the Google test unit.
    assert_eq!(
        cfg.resolve_unit(AdFormat::Rewarded),
        test_unit_id(AdFormat::Rewarded)
    );
    // use_test_ads overrides everything.
    let test_cfg = AdmobConfig::test_ads().with_unit(AdFormat::Interstitial, "ca-app-pub-x/y");
    assert_eq!(
        test_cfg.resolve_unit(AdFormat::Interstitial),
        test_unit_id(AdFormat::Interstitial)
    );
}

#[test]
fn consent_debug_overrides_are_disabled_for_production_ads() {
    let _guard = guarded();
    let mut app = build_app();
    app.insert_resource(AdmobConfig::default());
    app.insert_resource(UmpTestConfig {
        geography: Some(UmpDebugGeography::Eea),
        reset_consent_on_start: true,
    });
    app.update();

    assert_eq!(backend::ump_test_config(), (0, false));
}

/// Accumulates terminal outcomes across frames so assertions don't race the
/// double-buffered message swap (a real game would react frame-by-frame).
#[derive(Resource, Default)]
struct Recorder {
    loaded: u32,
    dismissed: u32,
    rewards: Vec<RewardEarned>,
}

fn record(
    mut rec: ResMut<Recorder>,
    mut loaded: MessageReader<AdLoaded>,
    mut dismissed: MessageReader<AdDismissed>,
    mut rewards: MessageReader<RewardEarned>,
) {
    rec.loaded += loaded.read().count() as u32;
    rec.dismissed += dismissed.read().count() as u32;
    for r in rewards.read() {
        rec.rewards.push(r.clone());
    }
}

fn show_rewarded_once(
    inv: Res<AdInventory>,
    mut shows: MessageWriter<ShowAd>,
    mut fired: Local<bool>,
) {
    if !*fired && inv.is_loaded(AdFormat::Rewarded) {
        *fired = true;
        shows.write(ShowAd(AdFormat::Rewarded));
    }
}

/// End-to-end against the fake: config → load → loaded → show → dismissed.
#[test]
fn interstitial_load_show_dismiss_flow() {
    let _guard = guarded();
    let mut app = build_app();
    app.init_resource::<Recorder>();
    app.insert_resource(AdmobConfig::test_ads());
    app.add_systems(Update, (load_interstitial_once, show_when_loaded, record));

    for _ in 0..10 {
        app.update();
    }

    let rec = app.world().resource::<Recorder>();
    assert_eq!(
        rec.loaded, 1,
        "interstitial should have reported loaded once"
    );
    assert_eq!(rec.dismissed, 1, "interstitial should have dismissed once");
    // Consumed after showing.
    assert_eq!(
        app.world()
            .resource::<AdInventory>()
            .state(AdFormat::Interstitial),
        AdLoadState::Idle
    );
}

/// A rewarded ad grants a reward when shown.
#[test]
fn rewarded_grants_reward() {
    let _guard = guarded();
    // SAFETY: `guarded()` serializes fake-driving tests, so this env state is
    // not observed by any concurrent test.
    unsafe {
        std::env::set_var("BEVY_ADMOB_FAKE_REWARD_AMOUNT", "7");
        std::env::set_var("BEVY_ADMOB_FAKE_REWARD_TYPE", "gems");
    }

    let mut app = build_app();
    app.init_resource::<Recorder>();
    app.insert_resource(AdmobConfig::test_ads());
    app.add_systems(
        Update,
        (
            |mut loads: MessageWriter<LoadAd>, mut fired: Local<bool>| {
                if !*fired {
                    *fired = true;
                    loads.write(LoadAd(AdFormat::Rewarded));
                }
            },
            show_rewarded_once,
            record,
        ),
    );

    for _ in 0..10 {
        app.update();
    }

    let rec = app.world().resource::<Recorder>();
    assert_eq!(
        rec.rewards.len(),
        1,
        "rewarded ad should grant exactly one reward"
    );
    assert_eq!(rec.rewards[0].amount, 7);
    assert_eq!(rec.rewards[0].reward_type, "gems");
    assert_eq!(rec.dismissed, 1, "rewarded ad should also dismiss");
}

/// A no-fill load surfaces as `Failed`, not `Loaded`.
#[test]
fn no_fill_reports_failure() {
    let _guard = guarded();
    unsafe { std::env::set_var("BEVY_ADMOB_FAKE_NO_FILL", "appopen") }

    let mut app = build_app();
    app.insert_resource(AdmobConfig::test_ads());
    app.add_systems(
        Update,
        |mut loads: MessageWriter<LoadAd>, mut fired: Local<bool>| {
            if !*fired {
                *fired = true;
                loads.write(LoadAd(AdFormat::AppOpen));
            }
        },
    );

    for _ in 0..6 {
        app.update();
    }

    assert_eq!(
        app.world()
            .resource::<AdInventory>()
            .state(AdFormat::AppOpen),
        AdLoadState::Failed
    );
}

#[test]
fn required_consent_is_collected_at_startup_without_game_requests() {
    let _guard = guarded();
    unsafe { std::env::set_var("BEVY_ADMOB_FAKE_CONSENT", "required") }

    let mut app = build_app();
    app.insert_resource(AdmobConfig::test_ads());

    app.update(); // init + first consent poll
    assert_eq!(
        app.world().resource::<AdmobState>().consent,
        ConsentStatus::Obtained
    );
    assert_eq!(backend::consent_presentations(), 1);

    app.world_mut()
        .resource_mut::<Messages<RequestConsent>>()
        .write(RequestConsent);
    app.update(); // pump request
    app.update(); // poll the new status

    assert_eq!(
        app.world().resource::<AdmobState>().consent,
        ConsentStatus::Obtained
    );
    assert!(app.world().resource::<AdmobState>().can_request_ads);
    assert_eq!(backend::consent_presentations(), 1);
}

#[test]
fn saved_approval_and_rejection_never_present_another_startup_form() {
    let _guard = guarded();
    for choice in ["approved", "rejected"] {
        backend::reset();
        unsafe {
            std::env::set_var("BEVY_ADMOB_FAKE_CONSENT", choice);
            std::env::set_var("BEVY_ADMOB_FAKE_CAN_REQUEST_ADS", "false");
        }
        let mut app = build_app();
        app.insert_resource(AdmobConfig::test_ads());
        app.update();
        app.world_mut().write_message(RequestConsent);
        app.update();
        let state = app.world().resource::<AdmobState>();
        assert_eq!(state.consent, ConsentStatus::Obtained);
        assert!(
            !state.can_request_ads,
            "an answered form does not infer ad readiness"
        );
        assert_eq!(backend::consent_presentations(), 0);
    }
}

#[test]
fn native_readiness_is_not_inferred_from_coarse_consent_status() {
    let _guard = guarded();
    unsafe {
        std::env::set_var("BEVY_ADMOB_FAKE_CONSENT", "unknown");
        std::env::set_var("BEVY_ADMOB_FAKE_CAN_REQUEST_ADS", "true");
    }

    let mut app = build_app();
    app.insert_resource(AdmobConfig::test_ads());
    app.update();

    let state = app.world().resource::<AdmobState>();
    assert_eq!(state.consent, ConsentStatus::Unknown);
    assert!(state.can_request_ads);
}

#[derive(Resource, Default)]
struct ConsentFailureRecorder(Vec<String>);

fn record_consent_failures(
    mut recorder: ResMut<ConsentFailureRecorder>,
    mut failures: MessageReader<ConsentInfoUpdateFailed>,
) {
    recorder
        .0
        .extend(failures.read().map(|failure| failure.error.clone()));
}

#[test]
fn consent_update_failure_is_observable_and_can_recover() {
    let _guard = guarded();
    unsafe {
        std::env::set_var("BEVY_ADMOB_FAKE_CONSENT", "unknown");
        std::env::set_var("BEVY_ADMOB_FAKE_CONSENT_UPDATE", "fail_once");
    }

    let mut app = build_app();
    app.init_resource::<ConsentFailureRecorder>()
        .insert_resource(AdmobConfig::test_ads())
        .add_systems(Update, record_consent_failures);
    app.update();
    app.update();

    assert!(!app.world().resource::<AdmobState>().can_request_ads);
    assert_eq!(app.world().resource::<ConsentFailureRecorder>().0.len(), 1);

    app.world_mut()
        .resource_mut::<Messages<RequestConsent>>()
        .write(RequestConsent);
    app.update();
    app.update();

    let state = app.world().resource::<AdmobState>();
    assert_eq!(state.consent, ConsentStatus::Obtained);
    assert!(state.can_request_ads);
}

#[test]
fn required_privacy_options_are_polled_and_presented_from_a_request() {
    let _guard = guarded();
    unsafe { std::env::set_var("BEVY_ADMOB_FAKE_PRIVACY_OPTIONS", "required") }

    let mut app = build_app();
    app.insert_resource(AdmobConfig::test_ads());
    app.update();

    assert_eq!(
        *app.world().resource::<PrivacyOptionsRequirement>(),
        PrivacyOptionsRequirement::Required
    );
    assert_eq!(backend::privacy_options_presentations(), 0);
    assert!(
        app.world()
            .resource::<PrivacyOptionsRequirement>()
            .is_required()
    );

    app.world_mut()
        .resource_mut::<Messages<PresentPrivacyOptions>>()
        .write(PresentPrivacyOptions);
    app.update();

    assert_eq!(backend::privacy_options_presentations(), 1);
    assert_eq!(
        *app.world().resource::<PrivacyOptionsRequirement>(),
        PrivacyOptionsRequirement::Required
    );
}

#[test]
fn privacy_options_are_not_required_without_a_fake_override() {
    let _guard = guarded();
    let mut app = build_app();
    app.insert_resource(AdmobConfig::test_ads());
    app.update();

    assert_eq!(
        *app.world().resource::<PrivacyOptionsRequirement>(),
        PrivacyOptionsRequirement::NotRequired
    );
    assert!(
        !app.world()
            .resource::<PrivacyOptionsRequirement>()
            .is_required()
    );
    assert!(!PrivacyOptionsRequirement::Unknown.is_required());
}

#[test]
fn every_debug_geography_and_reset_reach_the_backend_for_test_ads() {
    let _guard = guarded();
    for (geography, wire) in [
        (UmpDebugGeography::Eea, 1),
        (UmpDebugGeography::RegulatedUsState, 3),
        (UmpDebugGeography::Other, 4),
    ] {
        backend::reset();
        let mut app = build_app();
        app.insert_resource(AdmobConfig::test_ads());
        app.insert_resource(UmpTestConfig {
            geography: Some(geography),
            reset_consent_on_start: true,
        });
        app.update();
        assert_eq!(backend::ump_test_config(), (wire, true));
    }
}

#[test]
fn ump_qa_launch_reaches_the_backend_only_with_test_ads() {
    let _guard = guarded();
    let vars = |name: &str| match name {
        "GAME_UMP_QA" => Some("us".into()),
        "GAME_UMP_TEST_DEVICE_ID" => Some("HASHED".into()),
        _ => None,
    };
    let qa = UmpQa::from_vars(
        crate::environment::AppEnvironment::Development,
        "GAME_",
        vars,
    );
    for (mut config, wire) in [
        (AdmobConfig::test_ads(), (3, true)),
        (AdmobConfig::default(), (0, false)),
    ] {
        backend::reset();
        qa.apply(&mut config);
        let mut app = build_app();
        app.insert_resource(config).insert_resource(qa.test);
        app.update();
        assert_eq!(backend::ump_test_config(), wire);
        assert_eq!(backend::test_devices(), "HASHED");
    }
    backend::reset();
    let qa = UmpQa::from_vars(
        crate::environment::AppEnvironment::Production,
        "GAME_",
        vars,
    );
    let mut config = AdmobConfig::test_ads();
    qa.apply(&mut config);
    let mut app = build_app();
    app.insert_resource(config).insert_resource(qa.test);
    app.update();
    assert_eq!(backend::ump_test_config(), (0, false));
    assert_eq!(backend::test_devices(), "");
}

#[test]
fn banner_show_hide_toggles_state() {
    let _guard = guarded();
    let mut app = build_app();
    app.insert_resource(AdmobConfig::test_ads());

    app.update(); // init
    assert_eq!(app.world().resource::<AdmobState>().banner_height, 0.0);
    app.world_mut()
        .resource_mut::<Messages<ShowBanner>>()
        .write(ShowBanner::default());
    app.update();
    app.update(); // publish the asynchronous native result
    assert!(app.world().resource::<AdmobState>().banner_visible);
    assert_eq!(app.world().resource::<AdmobState>().banner_height, 50.0);

    app.world_mut()
        .resource_mut::<Messages<HideBanner>>()
        .write(HideBanner);
    app.update();
    app.update();
    assert!(!app.world().resource::<AdmobState>().banner_visible);
    assert_eq!(app.world().resource::<AdmobState>().banner_height, 0.0);
}

#[test]
fn banner_reserves_no_space_on_no_fill_or_failed_mount() {
    let _guard = guarded();
    let mut app = build_app();
    app.insert_resource(AdmobConfig::test_ads());
    app.update();
    unsafe { std::env::set_var("BEVY_ADMOB_FAKE_SHOW_FAIL", "banner") };
    app.world_mut().write_message(ShowBanner::default());
    app.update();
    assert_eq!(app.world().resource::<AdmobState>().banner_height, 0.0);
    assert!(!app.world().resource::<AdmobState>().banner_visible);

    unsafe {
        std::env::remove_var("BEVY_ADMOB_FAKE_SHOW_FAIL");
        std::env::set_var("BEVY_ADMOB_FAKE_NO_FILL", "banner");
    }
    app.world_mut().write_message(ShowBanner::default());
    app.update();
    assert_eq!(app.world().resource::<AdmobState>().banner_height, 0.0);
    assert!(!app.world().resource::<AdmobState>().banner_visible);
    app.world_mut().write_message(HideBanner);
    app.update();
    assert_eq!(app.world().resource::<AdmobState>().banner_height, 0.0);
}
#[test]
fn unavailable_try_show_is_dropped_and_never_shows_after_a_later_load() {
    let _guard = guarded();
    let mut app = build_app();
    app.insert_resource(AdmobConfig::test_ads());
    app.update();
    app.world_mut()
        .write_message(TryShowAd(AdFormat::Interstitial));
    app.update();
    assert!(app.world().resource::<Messages<AdShowFailed>>().is_empty());
    app.world_mut()
        .write_message(LoadAd(AdFormat::Interstitial));
    app.update();
    app.update();
    assert!(
        app.world()
            .resource::<AdInventory>()
            .is_loaded(AdFormat::Interstitial)
    );
    assert!(app.world().resource::<Messages<AdShown>>().is_empty());
    app.world_mut()
        .write_message(TryShowAd(AdFormat::Interstitial));
    app.world_mut()
        .write_message(TryShowAd(AdFormat::Interstitial));
    app.update();
    app.update(); // drain the native presentation callback
    assert_eq!(app.world().resource::<Messages<AdShown>>().len(), 1);
}
#[test]
fn default_config_starts_ads_without_game_owned_tracking_systems() {
    let _guard = guarded();
    crate::att::test_support::set_status(0);
    let mut app = build_app();
    app.insert_resource(AdmobConfig::test_ads());
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(crate::att::test_support::requests(), 1);
    assert_eq!(backend::initializations(), 1);
    assert!(app.world().resource::<AdmobState>().can_request_ads);
}

#[test]
fn ads_request_tracking_by_default_and_wait_for_the_result() {
    let _guard = guarded();
    crate::att::test_support::set_status(0);
    unsafe { std::env::set_var("BEVY_IOS_FAKE_ATT", "notdetermined") };
    let mut app = build_app();
    app.insert_resource(AdmobConfig::test_ads());
    for _ in 0..8 {
        app.update();
        assert!(!app.world().resource::<AdmobState>().initialized);
    }
    assert_eq!(crate::att::test_support::requests(), 1);
    assert_eq!(backend::initializations(), 0);
    // The native callback can resolve while no new Bevy request is sent.
    crate::att::test_support::set_status(2);
    app.update();
    assert!(app.world().resource::<AdmobState>().initialized);
    assert_eq!(backend::initializations(), 1);
    app.update();
    assert_eq!(backend::initializations(), 1);
}

#[test]
fn every_existing_att_decision_skips_the_prompt_and_starts_ads() {
    let _guard = guarded();
    for status in [1, 2, 3] {
        backend::reset();
        crate::att::test_support::set_status(status);
        let mut app = build_app();
        app.insert_resource(AdmobConfig::test_ads());
        app.update();
        assert!(app.world().resource::<AdmobState>().initialized);
        assert_eq!(crate::att::test_support::requests(), 0);
    }
}

#[test]
fn manual_prompt_and_absent_config_never_start_tracking_implicitly() {
    let _guard = guarded();
    crate::att::test_support::set_status(0);
    let mut app = build_app();
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(crate::att::test_support::requests(), 0);
    app.insert_resource(AdmobConfig {
        tracking_prompt: AdTrackingPrompt::Manual,
        ..AdmobConfig::test_ads()
    });
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(crate::att::test_support::requests(), 0);
    assert_eq!(backend::initializations(), 0);
    app.world_mut().write_message(RequestTracking);
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(crate::att::test_support::requests(), 1);
    assert!(app.world().resource::<AdmobState>().initialized);
}

#[test]
fn early_consent_survives_att_wait_but_ads_are_rejected_until_ready() {
    let _guard = guarded();
    crate::att::test_support::set_status(0);
    let mut app = build_app();
    app.insert_resource(AdmobConfig {
        tracking_prompt: AdTrackingPrompt::Manual,
        ..AdmobConfig::test_ads()
    });
    app.world_mut().write_message(RequestConsent);
    app.world_mut()
        .write_message(LoadAd(AdFormat::Interstitial));
    app.world_mut().write_message(ShowBanner::default());
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(backend::initializations(), 0);
    assert_eq!(
        app.world()
            .resource::<AdInventory>()
            .state(AdFormat::Interstitial),
        AdLoadState::Failed
    );
    assert!(!app.world().resource::<AdmobState>().banner_visible);
    unsafe { std::env::set_var("BEVY_ADMOB_FAKE_CONSENT", "required") };
    crate::att::test_support::set_status(2);
    app.update();
    assert_eq!(
        app.world().resource::<AdmobState>().consent,
        ConsentStatus::Obtained
    );
    assert!(app.world().resource::<AdmobState>().can_request_ads);
}

#[test]
fn ump_blocks_banner_and_fullscreen_requests_after_att_is_resolved() {
    let _guard = guarded();
    unsafe {
        std::env::set_var("BEVY_ADMOB_FAKE_CONSENT", "required");
        std::env::set_var("BEVY_ADMOB_FAKE_CONSENT_PENDING", "1");
    }
    let mut app = build_app();
    app.insert_resource(AdmobConfig::test_ads());
    app.update();
    app.world_mut()
        .write_message(LoadAd(AdFormat::Interstitial));
    app.world_mut()
        .write_message(ShowAd(AdFormat::Interstitial));
    app.world_mut().write_message(ShowBanner::default());
    app.update();
    assert_eq!(app.world().resource::<Messages<AdLoadFailed>>().len(), 1);
    assert_eq!(app.world().resource::<Messages<AdShowFailed>>().len(), 2);
    assert!(!app.world().resource::<AdmobState>().banner_visible);
    app.world_mut().write_message(RequestConsent);
    app.update();
    assert_eq!(
        backend::consent_presentations(),
        1,
        "pending forms coalesce"
    );
    assert!(!app.world().resource::<AdmobState>().can_request_ads);
    backend::complete_consent();
    app.update();
    app.world_mut()
        .write_message(LoadAd(AdFormat::Interstitial));
    app.world_mut().write_message(ShowBanner::default());
    app.update();
    app.update(); // poll the loaded ad and mounted banner
    assert!(
        app.world()
            .resource::<AdInventory>()
            .is_loaded(AdFormat::Interstitial)
    );
    assert!(app.world().resource::<AdmobState>().banner_visible);
}

#[test]
fn ios_plugin_and_ads_plugin_install_att_only_once() {
    let _guard = guarded();
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, crate::IosPlugin));
    assert!(app.is_plugin_added::<AttPlugin>());
}
