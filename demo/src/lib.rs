//! A button-per-feature demo of `bevy_ios_toolkit`. One tappable row per
//! integration, with a live status line at the top.
//!
//! On desktop it runs the integrations with meaningful built-in fakes; the
//! platform-owned StoreKit purchase surface is absent:
//!
//! ```text
//! cargo run --bin demo        # from this crate
//! ```
//!
//! On iOS the same [`run`] body drives the real StoreKit / AdMob / ATT / GameKit
//! bridges; the native shell (`ios/main.m`) calls [`main_rs`]. See `ios/README.md`.

use bevy::prelude::*;
use bevy_ios_toolkit::prelude::*;

#[cfg(target_os = "ios")]
const REMOVE_ADS: &str = "iap.playground.removeads";
const LEADERBOARD: &str = "lb.demo.highscore";
const ACHIEVEMENT: &str = "ach.demo.first_tap";

/// iOS entry point: the native shell (`ios/main.m`) calls this symbol; winit's
/// iOS backend then drives the UIApplication lifecycle from inside Bevy.
#[unsafe(no_mangle)]
pub extern "C" fn main_rs() {
    run();
}

/// Build and run the demo app. Called by the desktop binary and by [`main_rs`].
pub fn run() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "bevy_ios_toolkit demo".into(),
            // winit reads startup dimensions as points before it knows the
            // display scale. Query before a window or scene exists.
            #[cfg(target_os = "ios")]
            resolution: {
                let size = platform::screen_size();
                (size.x as u32, size.y as u32).into()
            },
            #[cfg(target_os = "ios")]
            prefers_home_indicator_hidden: true,
            #[cfg(target_os = "ios")]
            prefers_status_bar_hidden: true,
            ..default()
        }),
        ..default()
    }))
    .add_plugins(IosPlugin)
    .insert_resource(AdmobConfig::test_ads())
    .init_resource::<PendingShow>()
    .add_systems(Startup, setup)
    .add_systems(
        Update,
        (
            on_ads_button_press,
            on_platform_button_press,
            drive_pending,
            restyle_buttons,
            sync_privacy_options_button,
            update_status,
        ),
    );

    #[cfg(target_os = "ios")]
    app.insert_resource(StoreConfig {
        product_ids: vec![REMOVE_ADS.into()],
    })
    .init_resource::<LastStoreResult>()
    .add_systems(
        Update,
        (
            on_store_button_press,
            record_store_results,
            sync_store_button,
        ),
    );

    // UI regression runs use UMP's supported test geography so the banner
    // checks do not depend on the runner's location or cached consent.
    if std::env::var_os("BEVY_IOS_TOOLKIT_DEMO_QA").is_some() {
        app.insert_resource(UmpTestConfig {
            geography: Some(UmpDebugGeography::Other),
            reset_consent_on_start: true,
        });
    }
    app.run();
}

/// One button per feature. Full-screen ads load *and* present from a single tap.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Action {
    #[cfg(target_os = "ios")]
    StoreEnvironment,
    #[cfg(target_os = "ios")]
    Purchase,
    #[cfg(target_os = "ios")]
    Restore,
    Interstitial,
    Rewarded,
    ToggleBanner,
    Consent,
    PrivacyOptions,
    Tracking,
    Haptic,
    Share,
    Review,
    GameCenter,
    Notify,
}

const ROWS: &[(&str, Action)] = &[
    #[cfg(target_os = "ios")]
    ("Check App Store Environment", Action::StoreEnvironment),
    #[cfg(target_os = "ios")]
    ("Buy: Remove Ads", Action::Purchase),
    #[cfg(target_os = "ios")]
    ("Restore Purchases", Action::Restore),
    ("Interstitial Ad", Action::Interstitial),
    ("Rewarded Ad", Action::Rewarded),
    ("Toggle Banner", Action::ToggleBanner),
    ("Request Ad Consent", Action::Consent),
    ("Privacy Options", Action::PrivacyOptions),
    ("Request Tracking (ATT)", Action::Tracking),
    ("Haptic Tap", Action::Haptic),
    ("Share Result", Action::Share),
    ("Ask for Review", Action::Review),
    ("Game Center", Action::GameCenter),
    ("Notify in 10s", Action::Notify),
];

/// Full-screen ads a tap asked for; presented by `drive_pending` once loaded.
#[derive(Resource, Default)]
struct PendingShow(std::collections::HashSet<AdFormat>);

#[cfg(target_os = "ios")]
#[derive(Resource, Default)]
struct LastStoreResult(String);

#[derive(Component)]
struct StatusLine;

#[derive(Component)]
struct PrivacyOptionsButton;

#[cfg(target_os = "ios")]
#[derive(Component)]
struct PurchaseButtonLabel;

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: Val::Px(8.0),
            padding: UiRect::all(Val::Px(16.0)),
            ..default()
        })
        .with_children(|root| {
            root.spawn((
                StatusLine,
                Text::new("ready"),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.9, 1.0)),
                Node {
                    margin: UiRect::bottom(Val::Px(12.0)),
                    ..default()
                },
            ));
            for (label, action) in ROWS {
                let mut button = root.spawn((
                    *action,
                    Button,
                    Node {
                        width: Val::Px(320.0),
                        display: if *action == Action::PrivacyOptions {
                            Display::None
                        } else {
                            Display::Flex
                        },
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(Val::Px(16.0), Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(REST),
                ));
                if *action == Action::PrivacyOptions {
                    button.insert(PrivacyOptionsButton);
                }
                button.with_children(|b| {
                    #[cfg_attr(not(target_os = "ios"), allow(unused_variables))]
                    let label = b.spawn((
                        Text::new(*label),
                        TextFont {
                            font_size: FontSize::Px(18.0),
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                    #[cfg(target_os = "ios")]
                    if *action == Action::Purchase {
                        let mut label = label;
                        label.insert(PurchaseButtonLabel);
                    }
                });
            }
        });
}

const REST: Color = Color::srgb(0.16, 0.20, 0.32);
const HOVER: Color = Color::srgb(0.24, 0.30, 0.46);
const PRESS: Color = Color::srgb(0.36, 0.46, 0.70);

#[allow(clippy::type_complexity)]
fn restyle_buttons(
    mut buttons: Query<(&Interaction, &mut BackgroundColor), (Changed<Interaction>, With<Button>)>,
) {
    for (interaction, mut bg) in buttons.iter_mut() {
        bg.0 = match interaction {
            Interaction::Pressed => PRESS,
            Interaction::Hovered => HOVER,
            Interaction::None => REST,
        };
    }
}

/// Forward one explicit iOS store action while the native owner is idle.
#[cfg(target_os = "ios")]
#[allow(clippy::too_many_arguments)]
fn on_store_button_press(
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    activity: Res<StoreActivity>,
    products: Res<StoreProducts>,
    entitlements: Res<Entitlements>,
    mut purchase: MessageWriter<PurchaseRequest>,
    mut restore: MessageWriter<RestoreRequest>,
    mut environment: MessageWriter<RequestAppStoreEnvironment>,
) {
    for (interaction, action) in buttons.iter() {
        if *interaction != Interaction::Pressed || !activity.is_idle() {
            continue;
        }
        match action {
            Action::StoreEnvironment => {
                environment.write(RequestAppStoreEnvironment);
            }
            Action::Purchase
                if entitlements.is_ready()
                    && !entitlements.owns(REMOVE_ADS)
                    && products.get(REMOVE_ADS).is_some() =>
            {
                purchase.write(PurchaseRequest(REMOVE_ADS.into()));
            }
            Action::Purchase => {}
            Action::Restore => {
                restore.write(RestoreRequest);
            }
            _ => {}
        }
    }
}

#[cfg(target_os = "ios")]
fn record_store_results(
    mut last: ResMut<LastStoreResult>,
    mut purchases: MessageReader<PurchaseCompleted>,
    mut restores: MessageReader<RestoreCompleted>,
) {
    for purchase in purchases.read() {
        last.0 = format!("purchase {}: {:?}", purchase.product_id, purchase.outcome);
    }
    for restore in restores.read() {
        last.0 = format!("restore: {:?}", restore.outcome);
    }
}

#[cfg(target_os = "ios")]
fn sync_store_button(
    products: Res<StoreProducts>,
    entitlements: Res<Entitlements>,
    activity: Res<StoreActivity>,
    mut label: Query<&mut Text, With<PurchaseButtonLabel>>,
) {
    if !products.is_changed() && !entitlements.is_changed() && !activity.is_changed() {
        return;
    }
    let Ok(mut label) = label.single_mut() else {
        return;
    };
    label.0 = match &*activity {
        StoreActivity::Purchasing { .. } => "Purchasing…".into(),
        StoreActivity::Restoring => "Restoring…".into(),
        StoreActivity::Idle if entitlements.owns(REMOVE_ADS) => "Purchased".into(),
        StoreActivity::Idle if !entitlements.is_ready() => "Checking Purchases…".into(),
        StoreActivity::Idle => products.get(REMOVE_ADS).map_or_else(
            || "Store Unavailable".into(),
            |product| format!("Buy: Remove Ads — {}", product.display_price),
        ),
    };
}

/// Fan ad button presses out to the matching toolkit message.
#[allow(clippy::too_many_arguments)]
fn on_ads_button_press(
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    admob: Res<AdmobState>,
    privacy_options_requirement: Res<PrivacyOptionsRequirement>,
    inventory: Res<AdInventory>,
    mut pending: ResMut<PendingShow>,
    mut load: MessageWriter<LoadAd>,
    mut show: MessageWriter<ShowAd>,
    mut show_banner: MessageWriter<ShowBanner>,
    mut hide_banner: MessageWriter<HideBanner>,
    mut consent: MessageWriter<RequestConsent>,
    mut privacy_options: MessageWriter<PresentPrivacyOptions>,
) {
    for (interaction, action) in buttons.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            #[cfg(target_os = "ios")]
            Action::Purchase | Action::Restore => {}
            Action::Interstitial if admob.can_request_ads => queue_ad(
                AdFormat::Interstitial,
                &inventory,
                &mut pending,
                &mut load,
                &mut show,
            ),
            Action::Rewarded if admob.can_request_ads => queue_ad(
                AdFormat::Rewarded,
                &inventory,
                &mut pending,
                &mut load,
                &mut show,
            ),
            Action::ToggleBanner if admob.can_request_ads => {
                if admob.banner_visible {
                    hide_banner.write(HideBanner);
                } else {
                    show_banner.write(ShowBanner::default());
                }
            }
            Action::Consent => {
                consent.write(RequestConsent);
            }
            Action::PrivacyOptions
                if *privacy_options_requirement == PrivacyOptionsRequirement::Required =>
            {
                privacy_options.write(PresentPrivacyOptions);
            }
            Action::PrivacyOptions => {}
            _ => {}
        }
    }
}

/// Fan platform and Game Center presses out separately so this remains a valid
/// Bevy system as integrations are added to the demo.
#[allow(clippy::too_many_arguments)]
fn on_platform_button_press(
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    gc: Res<GameCenter>,
    permission: Res<NotificationPermission>,
    mut ask: MessageWriter<RequestNotificationPermission>,
    mut notify: MessageWriter<ScheduleNotification>,
    mut att: MessageWriter<RequestTracking>,
    mut auth: MessageWriter<AuthenticateGameCenter>,
    mut submit: MessageWriter<SubmitScore>,
    mut dashboard: MessageWriter<ShowGameCenter>,
    mut achievement: MessageWriter<ReportAchievement>,
) {
    for (interaction, action) in buttons.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            Action::Tracking => {
                att.write(RequestTracking);
            }
            Action::Haptic => {
                platform::haptics::play(Haptic::Medium);
            }
            Action::Share => {
                platform::share::text("bevy_ios_toolkit demo — 4200 points");
            }
            Action::Review => {
                review::request();
            }
            // First tap asks; once the answer is in, a tap schedules one ten
            // seconds out. Background the app to see it arrive.
            Action::Notify => {
                if permission.can_deliver() {
                    notify.write(ScheduleNotification {
                        id: "demo".into(),
                        title: "bevy_ios_toolkit".into(),
                        body: "Scheduled ten seconds ago, from Rust.".into(),
                        after: std::time::Duration::from_secs(10),
                    });
                } else {
                    ask.write(RequestNotificationPermission);
                }
            }
            // First tap signs in; once authenticated, a tap submits a score +
            // achievement and opens the dashboard — the whole feature in one button.
            Action::GameCenter => {
                if gc.is_authenticated() {
                    submit.write(SubmitScore {
                        leaderboard_id: LEADERBOARD.into(),
                        score: 4200,
                    });
                    achievement.write(ReportAchievement {
                        achievement_id: ACHIEVEMENT.into(),
                        percent: 100.0,
                    });
                    dashboard.write(ShowGameCenter);
                } else {
                    auth.write(AuthenticateGameCenter);
                }
            }
            _ => {}
        }
    }
}

fn sync_privacy_options_button(
    requirement: Res<PrivacyOptionsRequirement>,
    mut buttons: Query<&mut Node, With<PrivacyOptionsButton>>,
) {
    if !requirement.is_changed() {
        return;
    }
    let display = if *requirement == PrivacyOptionsRequirement::Required {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut buttons {
        node.display = display;
    }
}

/// One tap = load *and* present: show now if it's ready, otherwise load and let
/// [`drive_pending`] present it the moment it finishes loading.
fn queue_ad(
    format: AdFormat,
    inventory: &AdInventory,
    pending: &mut PendingShow,
    load: &mut MessageWriter<LoadAd>,
    show: &mut MessageWriter<ShowAd>,
) {
    if inventory.is_loaded(format) {
        show.write(ShowAd(format));
    } else {
        load.write(LoadAd(format));
        pending.0.insert(format);
    }
}

/// Present each tap-queued ad as soon as its load finishes (or drop it on failure).
fn drive_pending(
    mut pending: ResMut<PendingShow>,
    inventory: Res<AdInventory>,
    mut show: MessageWriter<ShowAd>,
) {
    pending.0.retain(|&format| match inventory.state(format) {
        AdLoadState::Loaded => {
            show.write(ShowAd(format));
            false
        }
        AdLoadState::Failed => false,
        _ => true,
    });
}

/// Reflect live state from the toolkit's resources into the status line.
#[allow(clippy::too_many_arguments)]
fn update_status(
    mut status: Query<&mut Text, With<StatusLine>>,
    #[cfg(target_os = "ios")] environment: Res<AppStoreEnvironment>,
    #[cfg(target_os = "ios")] entitlements: Res<Entitlements>,
    #[cfg(target_os = "ios")] activity: Res<StoreActivity>,
    #[cfg(target_os = "ios")] products: Res<StoreProducts>,
    #[cfg(target_os = "ios")] last_store_result: Res<LastStoreResult>,
    inventory: Res<AdInventory>,
    admob: Res<AdmobState>,
    privacy_options: Res<PrivacyOptionsRequirement>,
    att: Res<TrackingStatus>,
    gc: Res<GameCenter>,
    power: Res<PowerState>,
) {
    let Ok(mut text) = status.single_mut() else {
        return;
    };
    #[cfg(target_os = "ios")]
    let offer = products
        .get(REMOVE_ADS)
        .map_or("unavailable", |product| product.display_price.as_str());
    #[cfg(target_os = "ios")]
    let store = format!(
        "store: {} | price: {} | entitlements: {:?} | owned: {} | activity: {:?} | result: {} | ",
        *environment,
        offer,
        entitlements.state(),
        entitlements.owns(REMOVE_ADS),
        *activity,
        if last_store_result.0.is_empty() {
            "none"
        } else {
            &last_store_result.0
        },
    );
    #[cfg(not(target_os = "ios"))]
    let store = "";
    text.0 = format!(
        "{store}interstitial: {:?} | banner: {} ({:.0}pt) | consent: {:?} | ads-ready: {} | privacy: {:?} | att: {:?} | gc: {:?} | thermal: {:?}{}",
        inventory.state(AdFormat::Interstitial),
        admob.banner_visible,
        admob.banner_height,
        admob.consent,
        admob.can_request_ads,
        *privacy_options,
        *att,
        gc.auth,
        power.thermal,
        if power.low_power_mode {
            " (low power)"
        } else {
            ""
        },
    );
}
