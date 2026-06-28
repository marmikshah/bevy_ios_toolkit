//! Desktop walkthrough of the ad flow against the built-in fake backend.
//!
//! ```text
//! cargo run --example ads
//! BEVY_ADMOB_FAKE_NO_FILL=interstitial cargo run --example ads   # interstitial won't fill
//! BEVY_ADMOB_FAKE_REWARD_AMOUNT=50 cargo run --example ads        # bigger reward
//! BEVY_ADMOB_FAKE_CONSENT=required cargo run --example ads        # consent form path
//! ```
//!
//! It shows a banner, preloads + shows an interstitial and a rewarded ad,
//! prints every outcome, then exits.

use bevy::prelude::*;
use bevy_ios_toolkit::prelude::*;

fn main() {
    App::new()
        .add_plugins((MinimalPlugins, IosPlugin))
        // `test_ads()` serves Google's official sample units — zero setup.
        .insert_resource(AdmobConfig::test_ads())
        .add_systems(Startup, kick_off)
        .add_systems(
            Update,
            (
                show_when_ready,
                report_loaded.run_if(on_message::<AdLoaded>),
                report_load_failed.run_if(on_message::<AdLoadFailed>),
                report_shown.run_if(on_message::<AdShown>),
                report_dismissed.run_if(on_message::<AdDismissed>),
                report_show_failed.run_if(on_message::<AdShowFailed>),
                report_reward.run_if(on_message::<RewardEarned>),
                report_consent.run_if(on_message::<ConsentUpdated>),
                resolve_consent.run_if(on_message::<ConsentUpdated>),
                exit_after_settling,
            ),
        )
        .run();
}

/// Show a banner and start preloading the full-screen formats up front.
fn kick_off(mut loads: MessageWriter<LoadAd>, mut banner: MessageWriter<ShowBanner>) {
    banner.write(ShowBanner {
        position: BannerPosition::Bottom,
    });
    loads.write(LoadAd(AdFormat::Interstitial));
    loads.write(LoadAd(AdFormat::Rewarded));
}

/// Present each full-screen ad the first frame it reports loaded.
fn show_when_ready(
    inventory: Res<AdInventory>,
    mut shows: MessageWriter<ShowAd>,
    mut shown_interstitial: Local<bool>,
    mut shown_rewarded: Local<bool>,
) {
    if !*shown_interstitial && inventory.is_loaded(AdFormat::Interstitial) {
        *shown_interstitial = true;
        println!("showing interstitial");
        shows.write(ShowAd(AdFormat::Interstitial));
    }
    if !*shown_rewarded && inventory.is_loaded(AdFormat::Rewarded) {
        *shown_rewarded = true;
        println!("showing rewarded");
        shows.write(ShowAd(AdFormat::Rewarded));
    }
}

fn report_loaded(mut loaded: MessageReader<AdLoaded>) {
    for AdLoaded(format) in loaded.read() {
        println!("loaded {format:?}");
    }
}

fn report_load_failed(mut failed: MessageReader<AdLoadFailed>) {
    for f in failed.read() {
        println!("load failed {:?}: {}", f.format, f.error);
    }
}

fn report_shown(mut shown: MessageReader<AdShown>) {
    for AdShown(format) in shown.read() {
        println!("shown {format:?}");
    }
}

fn report_dismissed(mut dismissed: MessageReader<AdDismissed>) {
    for AdDismissed(format) in dismissed.read() {
        println!("dismissed {format:?}");
    }
}

fn report_show_failed(mut failed: MessageReader<AdShowFailed>) {
    for f in failed.read() {
        println!("show failed {:?}: {}", f.format, f.error);
    }
}

fn report_reward(mut rewards: MessageReader<RewardEarned>) {
    for r in rewards.read() {
        println!(
            "REWARD: {} {} (from {:?})",
            r.amount, r.reward_type, r.format
        );
    }
}

fn report_consent(mut updated: MessageReader<ConsentUpdated>) {
    for ConsentUpdated(status) in updated.read() {
        println!("consent: {status:?}");
    }
}

/// When consent is required, present the form (the fake resolves it instantly).
fn resolve_consent(
    state: Res<AdmobState>,
    mut request: MessageWriter<RequestConsent>,
    mut asked: Local<bool>,
) {
    if !*asked && state.consent == ConsentStatus::Required {
        *asked = true;
        println!("consent required -> presenting form");
        request.write(RequestConsent);
    }
}

/// Give the flow a handful of frames to settle, then quit.
fn exit_after_settling(mut frames: Local<u32>, mut exit: MessageWriter<AppExit>) {
    *frames += 1;
    if *frames > 12 {
        exit.write(AppExit::Success);
    }
}
