//! Opt-in declarative banners. Games set eligibility; this plugin owns requests,
//! cancellation, lifecycle, adaptive width changes and bounded retry backoff.

use std::time::Duration;

use bevy::{
    prelude::*,
    window::{AppLifecycle, WindowEvent},
};

use super::{
    AdFormat, AdLoadFailed, AdShowFailed, AdmobState, BannerPosition, HideBanner, ShowBanner,
};

/// A banner placement's current eligibility. Native rendering stays hidden
/// until a creative arrives, so loading and no-fill reserve no layout space.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct BannerIntent {
    pub visible: bool,
    pub position: BannerPosition,
}

pub struct BannerPlugin;

impl Plugin for BannerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BannerIntent>()
            .init_resource::<AdmobState>()
            .init_resource::<Time<Real>>()
            .add_message::<WindowEvent>()
            .add_message::<ShowBanner>()
            .add_message::<HideBanner>()
            .add_message::<AdLoadFailed>()
            .add_message::<AdShowFailed>()
            .add_systems(Last, reconcile);
    }
}

const RETRY_MIN: Duration = Duration::from_secs(30);
const RETRY_MAX: Duration = Duration::from_secs(120);
const LOAD_WAIT: Duration = Duration::from_secs(60);

struct Banner {
    requested: Option<BannerPosition>,
    deadline: Duration,
    retry_at: Duration,
    retry_delay: Duration,
    focused: bool,
    suspended: bool,
    width: Option<f32>,
    resize_at: Option<Duration>,
}

impl Default for Banner {
    fn default() -> Self {
        Self {
            requested: None,
            deadline: Duration::ZERO,
            retry_at: Duration::ZERO,
            retry_delay: RETRY_MIN,
            focused: true,
            suspended: false,
            width: None,
            resize_at: None,
        }
    }
}

impl Banner {
    fn observe(&mut self, event: &WindowEvent, now: Duration, ios: bool) {
        match event {
            WindowEvent::WindowFocused(event) => self.focused = event.focused,
            WindowEvent::AppLifecycle(AppLifecycle::WillSuspend | AppLifecycle::Suspended) => {
                self.suspended = true
            }
            WindowEvent::AppLifecycle(AppLifecycle::Running) => {
                self.suspended = false;
                // UIKit becoming active is independent of winit's key-window
                // callback. System prompts may return without that callback.
                if ios {
                    self.focused = true;
                }
            }
            WindowEvent::AppLifecycle(AppLifecycle::WillResume) => self.suspended = false,
            WindowEvent::WindowResized(event) if event.width > 0.0 => {
                if self
                    .width
                    .is_some_and(|width| (width - event.width).abs() > 1.0)
                {
                    self.resize_at = Some(now + Duration::from_millis(300));
                }
                self.width = Some(event.width);
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn reconcile(
    time: Res<Time<Real>>,
    intent: Res<BannerIntent>,
    admob: Res<AdmobState>,
    mut events: MessageReader<WindowEvent>,
    mut load_failures: MessageReader<AdLoadFailed>,
    mut show_failures: MessageReader<AdShowFailed>,
    mut show: MessageWriter<ShowBanner>,
    mut hide: MessageWriter<HideBanner>,
    mut banner: Local<Banner>,
) {
    let now = time.elapsed();
    for event in events.read() {
        banner.observe(event, now, cfg!(target_os = "ios"));
    }
    let failed = load_failures.read().fold(false, |found, failure| {
        found | (failure.format == AdFormat::Banner)
    }) | show_failures.read().fold(false, |found, failure| {
        found | (failure.format == AdFormat::Banner)
    });
    let wanted = intent.visible && admob.can_request_ads && banner.focused && !banner.suspended;
    if !wanted {
        if banner.requested.take().is_some() {
            hide.write(HideBanner);
        }
        banner.retry_at = Duration::ZERO;
        banner.retry_delay = RETRY_MIN;
        banner.resize_at = None;
        return;
    }
    if banner.requested.is_some() {
        if failed || (!admob.banner_visible && now >= banner.deadline) {
            hide.write(HideBanner);
            banner.requested = None;
            banner.retry_at = now + banner.retry_delay;
            banner.retry_delay = (banner.retry_delay * 2).min(RETRY_MAX);
            // Process cancellation before a resize can issue a replacement.
            // Native commands are asynchronous; a same-frame hide would cancel it.
            return;
        } else if admob.banner_visible {
            banner.retry_delay = RETRY_MIN;
        }
    }
    let resized = banner.resize_at.is_some_and(|at| now >= at);
    if banner
        .requested
        .is_some_and(|position| position != intent.position)
        || resized
    {
        banner.requested = None;
        banner.retry_at = Duration::ZERO;
        banner.resize_at = None;
    }
    if banner.requested.is_none() && now >= banner.retry_at {
        show.write(ShowBanner {
            position: intent.position,
        });
        banner.requested = Some(intent.position);
        banner.deadline = now + LOAD_WAIT;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(BannerPlugin);
        app.world_mut().resource_mut::<BannerIntent>().visible = true;
        app
    }

    fn requests(app: &mut App) -> (usize, usize) {
        (
            app.world_mut()
                .resource_mut::<Messages<ShowBanner>>()
                .drain()
                .count(),
            app.world_mut()
                .resource_mut::<Messages<HideBanner>>()
                .drain()
                .count(),
        )
    }

    #[test]
    fn readiness_and_cancellation_do_not_wait_for_a_visible_banner() {
        let mut app = app();
        app.update();
        assert_eq!(requests(&mut app), (0, 0));
        app.world_mut().resource_mut::<AdmobState>().can_request_ads = true;
        app.update();
        assert_eq!(requests(&mut app), (1, 0));
        for _ in 0..10 {
            app.update();
        }
        assert_eq!(requests(&mut app), (0, 0));
        app.world_mut().resource_mut::<BannerIntent>().visible = false;
        app.update();
        assert_eq!(requests(&mut app), (0, 1));
        app.update();
        assert_eq!(requests(&mut app), (0, 0));
    }

    #[test]
    fn no_fill_retries_with_backoff_and_preserves_gameplay_frames() {
        let mut app = app();
        app.world_mut().resource_mut::<AdmobState>().can_request_ads = true;
        app.update();
        assert_eq!(requests(&mut app), (1, 0));
        for delay in [30, 60, 120, 120] {
            app.world_mut().write_message(AdLoadFailed {
                format: AdFormat::Banner,
                error: "no fill".into(),
            });
            app.update();
            assert_eq!(requests(&mut app), (0, 1));
            app.world_mut()
                .resource_mut::<Time<Real>>()
                .advance_by(Duration::from_secs(delay - 1));
            app.update();
            assert_eq!(requests(&mut app), (0, 0));
            app.world_mut()
                .resource_mut::<Time<Real>>()
                .advance_by(Duration::from_secs(1));
            app.update();
            assert_eq!(requests(&mut app), (1, 0));
        }
    }

    #[test]
    fn background_cancels_loading_and_return_can_request_again() {
        let mut app = app();
        app.world_mut().resource_mut::<AdmobState>().can_request_ads = true;
        app.update();
        assert_eq!(requests(&mut app), (1, 0));
        app.world_mut()
            .write_message(WindowEvent::AppLifecycle(AppLifecycle::Suspended));
        app.update();
        assert_eq!(requests(&mut app), (0, 1));
        app.world_mut()
            .write_message(WindowEvent::AppLifecycle(AppLifecycle::Running));
        app.update();
        assert_eq!(requests(&mut app), (1, 0));
    }

    #[test]
    fn ios_active_notification_recovers_focus_after_a_system_prompt() {
        for ios in [false, true] {
            let mut banner = Banner::default();
            banner.observe(
                &WindowEvent::WindowFocused(bevy::window::WindowFocused {
                    window: Entity::PLACEHOLDER,
                    focused: false,
                }),
                Duration::ZERO,
                ios,
            );
            banner.observe(
                &WindowEvent::AppLifecycle(AppLifecycle::Suspended),
                Duration::ZERO,
                ios,
            );
            assert!(!banner.focused && banner.suspended);
            banner.observe(
                &WindowEvent::AppLifecycle(AppLifecycle::WillResume),
                Duration::ZERO,
                ios,
            );
            assert!(!banner.focused);
            banner.observe(
                &WindowEvent::AppLifecycle(AppLifecycle::Running),
                Duration::ZERO,
                ios,
            );
            assert!(!banner.suspended);
            assert_eq!(banner.focused, ios);
        }
    }

    #[test]
    fn a_missing_callback_has_a_bounded_load_deadline() {
        let mut app = app();
        app.world_mut().resource_mut::<AdmobState>().can_request_ads = true;
        app.update();
        requests(&mut app);
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(LOAD_WAIT);
        app.update();
        assert_eq!(requests(&mut app), (0, 1));
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(RETRY_MIN);
        app.update();
        assert_eq!(requests(&mut app), (1, 0));
    }

    #[test]
    fn width_changes_coalesce_until_the_adaptive_size_settles() {
        let mut app = app();
        app.world_mut().resource_mut::<AdmobState>().can_request_ads = true;
        let resize = |width| {
            WindowEvent::WindowResized(bevy::window::WindowResized {
                window: Entity::PLACEHOLDER,
                width,
                height: 800.0,
            })
        };
        app.world_mut().write_message(resize(320.0));
        app.update();
        assert_eq!(requests(&mut app), (1, 0));
        app.world_mut().resource_mut::<AdmobState>().banner_visible = true;
        app.world_mut().write_message(resize(400.0));
        app.update();
        assert_eq!(requests(&mut app), (0, 0));
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_millis(200));
        app.world_mut().write_message(resize(450.0));
        app.update();
        assert_eq!(requests(&mut app), (0, 0));
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_millis(299));
        app.update();
        assert_eq!(requests(&mut app), (0, 0));
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_millis(1));
        app.update();
        assert_eq!(requests(&mut app), (1, 0));
    }

    #[test]
    fn failure_and_resize_cancel_before_requesting_a_replacement() {
        let mut app = app();
        app.world_mut().resource_mut::<AdmobState>().can_request_ads = true;
        for width in [320.0, 400.0] {
            app.world_mut().write_message(WindowEvent::WindowResized(
                bevy::window::WindowResized {
                    window: Entity::PLACEHOLDER,
                    width,
                    height: 800.0,
                },
            ));
            app.update();
            requests(&mut app);
        }
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_millis(300));
        app.world_mut().write_message(AdLoadFailed {
            format: AdFormat::Banner,
            error: "no fill".into(),
        });
        app.update();
        assert_eq!(requests(&mut app), (0, 1));
        app.update();
        assert_eq!(requests(&mut app), (1, 0));
    }
}
