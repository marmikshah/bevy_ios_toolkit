//! A button-per-feature demo of `bevy_ios_toolkit`. One tappable row per
//! integration, with a live status line at the top.
//!
//! On desktop it runs against the built-in fakes:
//!
//! ```text
//! cargo run --bin demo        # from this crate
//! ```
//!
//! On iOS the same [`run`] body drives the real StoreKit / AdMob / ATT / GameKit
//! bridges; the native shell (`ios/main.m`) calls [`main_rs`]. See `ios/README.md`.

use bevy::prelude::*;
use bevy_ios_toolkit::prelude::*;

const REMOVE_ADS: &str = "com.example.iostoolkitdemo.removeads";
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
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "bevy_ios_toolkit demo".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(IosPlugin)
        .insert_resource(StoreConfig {
            product_ids: vec![REMOVE_ADS.into()],
        })
        .insert_resource(AdmobConfig::test_ads())
        .add_systems(Startup, setup)
        .add_systems(Update, (on_button_press, restyle_buttons, update_status))
        .run();
}

/// What each button does when tapped.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Action {
    Purchase,
    Restore,
    LoadInterstitial,
    ShowInterstitial,
    LoadRewarded,
    ShowRewarded,
    ToggleBanner,
    Haptic,
    RequestAtt,
    Review,
    AuthGameCenter,
    SubmitScore,
    ShowGameCenter,
    RequestConsent,
}

const ROWS: &[(&str, Action)] = &[
    ("Buy: Remove Ads", Action::Purchase),
    ("Restore Purchases", Action::Restore),
    ("Load Interstitial", Action::LoadInterstitial),
    ("Show Interstitial", Action::ShowInterstitial),
    ("Load Rewarded", Action::LoadRewarded),
    ("Show Rewarded", Action::ShowRewarded),
    ("Toggle Banner", Action::ToggleBanner),
    ("Haptic Tap", Action::Haptic),
    ("Request Tracking (ATT)", Action::RequestAtt),
    ("Ask for Review", Action::Review),
    ("Game Center: Sign In", Action::AuthGameCenter),
    ("Submit Score 4200", Action::SubmitScore),
    ("Show Game Center", Action::ShowGameCenter),
    ("Request Ad Consent", Action::RequestConsent),
];

#[derive(Component)]
struct StatusLine;

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
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.9, 1.0)),
                Node {
                    margin: UiRect::bottom(Val::Px(12.0)),
                    ..default()
                },
            ));
            for (label, action) in ROWS {
                root.spawn((
                    *action,
                    Button,
                    Node {
                        width: Val::Px(320.0),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(Val::Px(16.0), Val::Px(10.0)),
                        ..default()
                    },
                    BackgroundColor(REST),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(*label),
                        TextFont {
                            font_size: 18.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
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

/// Fan a button press out to the matching toolkit message / call.
#[allow(clippy::too_many_arguments)]
fn on_button_press(
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    state: Res<AdmobState>,
    mut purchase: MessageWriter<PurchaseRequest>,
    mut restore: MessageWriter<RestoreRequest>,
    mut load: MessageWriter<LoadAd>,
    mut show: MessageWriter<ShowAd>,
    mut show_banner: MessageWriter<ShowBanner>,
    mut hide_banner: MessageWriter<HideBanner>,
    mut consent: MessageWriter<RequestConsent>,
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
            Action::Purchase => {
                purchase.write(PurchaseRequest(REMOVE_ADS.into()));
            }
            Action::Restore => {
                restore.write(RestoreRequest);
            }
            Action::LoadInterstitial => {
                load.write(LoadAd(AdFormat::Interstitial));
            }
            Action::ShowInterstitial => {
                show.write(ShowAd(AdFormat::Interstitial));
            }
            Action::LoadRewarded => {
                load.write(LoadAd(AdFormat::Rewarded));
            }
            Action::ShowRewarded => {
                show.write(ShowAd(AdFormat::Rewarded));
            }
            Action::ToggleBanner => {
                if state.banner_visible {
                    hide_banner.write(HideBanner);
                } else {
                    show_banner.write(ShowBanner::default());
                }
            }
            Action::Haptic => {
                platform::haptics::play(Haptic::Medium);
            }
            Action::RequestAtt => {
                att.write(RequestTracking);
            }
            Action::Review => {
                review::request();
            }
            Action::AuthGameCenter => {
                auth.write(AuthenticateGameCenter);
            }
            Action::SubmitScore => {
                submit.write(SubmitScore {
                    leaderboard_id: LEADERBOARD.into(),
                    score: 4200,
                });
                achievement.write(ReportAchievement {
                    achievement_id: ACHIEVEMENT.into(),
                    percent: 100.0,
                });
            }
            Action::ShowGameCenter => {
                dashboard.write(ShowGameCenter);
            }
            Action::RequestConsent => {
                consent.write(RequestConsent);
            }
        }
    }
}

/// Reflect live state from the toolkit's resources into the status line.
fn update_status(
    mut status: Query<&mut Text, With<StatusLine>>,
    entitlements: Res<Entitlements>,
    inventory: Res<AdInventory>,
    admob: Res<AdmobState>,
    att: Res<TrackingStatus>,
    gc: Res<GameCenter>,
) {
    let Ok(mut text) = status.single_mut() else {
        return;
    };
    let owns = entitlements.owns(REMOVE_ADS);
    text.0 = format!(
        "ads-removed: {owns} | interstitial: {:?} | banner: {} | consent: {:?} | att: {:?} | gc: {:?}",
        inventory.state(AdFormat::Interstitial),
        admob.banner_visible,
        admob.consent,
        *att,
        gc.auth,
    );
}
