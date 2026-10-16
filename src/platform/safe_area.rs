//! The device's safe-area insets (notch / Dynamic Island, home indicator,
//! rounded corners), in points.
//!
//! Returns the raw insets, or `0.0` when unavailable (off-iOS, or before the key
//! window exists). The caller owns the fallback policy — a layout typically
//! clamps to a notch-class constant when an inset reports `0.0` and adds its own
//! breathing-room margin.
//!
//! Each [`insets`] call queues main-thread refreshes on iOS. Systems should read
//! the [`SafeArea`] resource instead: [`PlatformPlugin`](super::PlatformPlugin)
//! refreshes it in `PreUpdate`, quickly after window, focus and lifecycle
//! changes and then every two seconds.

use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::{AppLifecycle, WindowCreated, WindowFocused, WindowResized};

/// The four safe-area insets, in points. A portrait game usually needs only
/// [`top`](Self::top); landscape games and bottom-anchored HUD/toasts want all
/// four (home indicator, rounded corners).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    /// Notch / Dynamic Island edge.
    pub top: f32,
    /// Home-indicator edge.
    pub bottom: f32,
    /// Left rounded-corner / sensor edge (landscape).
    pub left: f32,
    /// Right rounded-corner / sensor edge (landscape).
    pub right: f32,
}

#[cfg(target_os = "ios")]
pub fn insets() -> Insets {
    unsafe extern "C" {
        fn platform_safe_top() -> f32;
        fn platform_safe_bottom() -> f32;
        fn platform_safe_left() -> f32;
        fn platform_safe_right() -> f32;
    }
    unsafe {
        Insets {
            top: platform_safe_top(),
            bottom: platform_safe_bottom(),
            left: platform_safe_left(),
            right: platform_safe_right(),
        }
    }
}

#[cfg(not(target_os = "ios"))]
pub fn insets() -> Insets {
    Insets::default()
}

/// The top safe-area inset alone — a convenience for the common portrait case.
/// Equivalent to `insets().top`, but a single FFI call.
#[cfg(target_os = "ios")]
pub fn top() -> f32 {
    unsafe extern "C" {
        fn platform_safe_top() -> f32;
    }
    unsafe { platform_safe_top() }
}

#[cfg(not(target_os = "ios"))]
pub fn top() -> f32 {
    0.0
}

/// The safe-area insets and window size in points, shared by every reader.
///
/// Kept current by [`PlatformPlugin`](super::PlatformPlugin) and written only
/// when a value changes, so `Res<SafeArea>::is_changed` marks real layout
/// changes. Insets stay zero off iOS and until UIKit installs the key window.
/// Values are UIKit points, before any `UiScale`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct SafeArea {
    pub insets: Insets,
    /// The key window's size, as [`screen_size`](super::screen_size) reports
    /// it. Zero off iOS.
    pub size: Vec2,
}

impl SafeArea {
    /// [`size`](Self::size) when it is a usable measurement (finite and over
    /// one point on both axes).
    pub fn measured_size(&self) -> Option<Vec2> {
        let size = self.size;
        (size.is_finite() && size.x > 1.0 && size.y > 1.0).then_some(size)
    }
}

/// Where [`refresh`] reads from: the native bridge, or a test double.
#[derive(Resource, Clone, Copy)]
pub(crate) struct SafeAreaSource(pub(crate) fn() -> SafeArea);

impl Default for SafeAreaSource {
    fn default() -> Self {
        Self(|| SafeArea {
            insets: insets(),
            size: super::screen_size(),
        })
    }
}

const RETRY_INTERVAL: Duration = Duration::from_millis(250);
const IDLE_INTERVAL: Duration = Duration::from_secs(2);
const RETRIES: u8 = 12;

/// Polls quickly for a short settling window after events that can change
/// UIKit's key window or geometry, then falls back to a coarse safety poll.
/// Startup retries matter: the first Bevy update can precede installation of
/// the key UIWindow. The safety poll catches UIKit-only inset changes that do
/// not produce a Bevy window event.
#[derive(Resource, Debug)]
pub(crate) struct SafeAreaPolling {
    next_poll: Duration,
    remaining: u8,
}

impl Default for SafeAreaPolling {
    fn default() -> Self {
        Self {
            next_poll: Duration::ZERO,
            remaining: RETRIES,
        }
    }
}

impl SafeAreaPolling {
    fn request(&mut self, now: Duration) {
        // An idle poll may be seconds away, so lifecycle/window changes get an
        // immediate read. Repeated resize events during the settling burst do
        // not defeat its rate limit.
        if self.remaining == 0 {
            self.next_poll = now;
        }
        self.remaining = RETRIES;
    }

    fn take_due(&mut self, now: Duration) -> bool {
        if now < self.next_poll {
            return false;
        }
        let interval = if self.remaining == 0 {
            IDLE_INTERVAL
        } else {
            self.remaining -= 1;
            if self.remaining == 0 {
                IDLE_INTERVAL
            } else {
                RETRY_INTERVAL
            }
        };
        self.next_poll = now + interval;
        true
    }
}

#[derive(SystemParam)]
pub(crate) struct RefreshEvents<'w, 's> {
    created: MessageReader<'w, 's, WindowCreated>,
    resized: MessageReader<'w, 's, WindowResized>,
    focused: MessageReader<'w, 's, WindowFocused>,
    lifecycle: MessageReader<'w, 's, AppLifecycle>,
}

impl RefreshEvents<'_, '_> {
    fn requested(&mut self) -> bool {
        let created = self.created.read().next().is_some();
        let resized = self.resized.read().next().is_some();
        let focused = self.focused.read().any(|event| event.focused);
        let resumed = self
            .lifecycle
            .read()
            .any(|event| matches!(event, AppLifecycle::Running | AppLifecycle::WillResume));
        created || resized || focused || resumed
    }
}

pub(crate) fn install(app: &mut App) {
    // Registered here too so the plugin works without `WindowPlugin`.
    app.add_message::<WindowCreated>()
        .add_message::<WindowResized>()
        .add_message::<WindowFocused>()
        .add_message::<AppLifecycle>()
        .init_resource::<SafeArea>()
        .init_resource::<SafeAreaSource>()
        .init_resource::<SafeAreaPolling>()
        .add_systems(PreUpdate, refresh);
}

fn refresh(
    time: Option<Res<Time<Real>>>,
    source: Res<SafeAreaSource>,
    mut polling: ResMut<SafeAreaPolling>,
    mut events: RefreshEvents,
    mut current: ResMut<SafeArea>,
) {
    let requested = events.requested();
    // Without a clock there is no rate limit to keep: read every frame.
    if let Some(time) = time {
        let now = time.elapsed();
        if requested {
            polling.request(now);
        }
        if !polling.take_due(now) {
            return;
        }
    }
    current.set_if_neq((source.0)());
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::sync::Mutex;

    #[test]
    fn startup_poll_is_immediate_then_rate_limited() {
        let mut polling = SafeAreaPolling::default();
        assert!(polling.take_due(Duration::ZERO));
        assert!(!polling.take_due(RETRY_INTERVAL / 2));
        assert!(polling.take_due(RETRY_INTERVAL));
    }

    #[test]
    fn polling_falls_back_to_a_coarse_interval_after_its_retry_budget() {
        let mut polling = SafeAreaPolling::default();
        for tick in 0..RETRIES {
            assert!(polling.take_due(RETRY_INTERVAL * u32::from(tick)));
        }
        let last_retry = RETRY_INTERVAL * u32::from(RETRIES - 1);
        assert!(!polling.take_due(last_retry + IDLE_INTERVAL / 2));
        assert!(polling.take_due(last_retry + IDLE_INTERVAL));
    }

    #[test]
    fn relevant_event_restarts_polling_immediately() {
        let mut polling = SafeAreaPolling::default();
        for tick in 0..RETRIES {
            assert!(polling.take_due(RETRY_INTERVAL * u32::from(tick)));
        }
        let resumed_at = Duration::from_secs(10);
        polling.request(resumed_at);
        assert!(polling.take_due(resumed_at));
        assert!(!polling.take_due(resumed_at + RETRY_INTERVAL / 2));
    }

    #[test]
    fn repeated_events_do_not_bypass_the_retry_rate_limit() {
        let mut polling = SafeAreaPolling::default();
        assert!(polling.take_due(Duration::ZERO));
        for milliseconds in 1..250 {
            let now = Duration::from_millis(milliseconds);
            polling.request(now);
            assert!(!polling.take_due(now));
        }
        assert!(polling.take_due(RETRY_INTERVAL));
    }

    #[test]
    fn measured_size_rejects_unset_and_degenerate_sizes() {
        let area = |size| SafeArea { size, ..default() };
        assert_eq!(area(Vec2::ZERO).measured_size(), None);
        assert_eq!(area(Vec2::new(1.0, 800.0)).measured_size(), None);
        assert_eq!(area(Vec2::new(f32::NAN, 800.0)).measured_size(), None);
        assert_eq!(
            area(Vec2::new(390.0, 844.0)).measured_size(),
            Some(Vec2::new(390.0, 844.0))
        );
    }

    #[test]
    fn desktop_plugin_reports_zero_insets() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(crate::platform::PlatformPlugin);
        app.update();
        assert_eq!(*app.world().resource::<SafeArea>(), SafeArea::default());
    }

    // The test source is a plain `fn`, so its readings live in a static.
    static READING: Mutex<(SafeArea, u32)> = Mutex::new((
        SafeArea {
            insets: Insets {
                top: 0.0,
                bottom: 0.0,
                left: 0.0,
                right: 0.0,
            },
            size: Vec2::ZERO,
        },
        0,
    ));

    fn fake_source() -> SafeArea {
        let mut reading = READING.lock().unwrap();
        reading.1 += 1;
        reading.0
    }

    fn set_reading(top: f32) {
        READING.lock().unwrap().0 = SafeArea {
            insets: Insets {
                top,
                bottom: 34.0,
                ..default()
            },
            size: Vec2::new(390.0, 844.0),
        };
    }

    #[derive(Resource, Default)]
    struct Changes(u32);

    fn reads() -> u32 {
        READING.lock().unwrap().1
    }

    fn advance(app: &mut App, by: Duration) {
        app.insert_resource(TimeUpdateStrategy::ManualDuration(by));
        app.update();
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    }

    #[test]
    fn resource_follows_window_events_and_writes_only_on_change() {
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin);
        app.add_plugins(crate::platform::PlatformPlugin)
            .insert_resource(SafeAreaSource(fake_source));
        // Drive real time by hand; TimePlugin would otherwise use the clock.
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        app.init_resource::<Changes>().add_systems(
            Update,
            |mut changes: ResMut<Changes>, area: Res<SafeArea>| {
                if area.is_changed() {
                    changes.0 += 1;
                }
            },
        );

        // Before the key window exists UIKit reports zero; the startup
        // retries pick up the inset once it appears.
        app.update();
        let startup = reads();
        assert!(startup >= 1);
        assert_eq!(app.world().resource::<SafeArea>().insets.top, 0.0);
        set_reading(59.0);
        app.update();
        assert_eq!(reads(), startup, "a second read inside 250 ms");
        advance(&mut app, RETRY_INTERVAL);
        assert_eq!(app.world().resource::<SafeArea>().insets.top, 59.0);
        assert_eq!(
            app.world().resource::<SafeArea>().measured_size(),
            Some(Vec2::new(390.0, 844.0))
        );

        // Exhaust the settling burst; the next idle poll is seconds away.
        for _ in 0..RETRIES {
            advance(&mut app, RETRY_INTERVAL);
        }
        let idle = reads();
        advance(&mut app, RETRY_INTERVAL);
        assert_eq!(reads(), idle);

        // A rotation arrives as a resize and is read on the same frame.
        set_reading(0.0);
        app.world_mut().write_message(WindowResized {
            window: Entity::PLACEHOLDER,
            width: 844.0,
            height: 390.0,
        });
        app.update();
        assert_eq!(reads(), idle + 1);
        assert_eq!(app.world().resource::<SafeArea>().insets.top, 0.0);

        // Unchanged readings never mark the resource changed.
        let changes = app.world().resource::<Changes>().0;
        advance(&mut app, IDLE_INTERVAL);
        advance(&mut app, IDLE_INTERVAL);
        assert_eq!(app.world().resource::<Changes>().0, changes);
        assert_eq!(reads(), idle + 3);
    }

    #[test]
    fn focus_and_resume_request_a_refresh_but_blur_does_not() {
        let mut app = App::new();
        app.add_message::<WindowCreated>()
            .add_message::<WindowResized>()
            .add_message::<WindowFocused>()
            .add_message::<AppLifecycle>();
        let requested = |app: &mut App| {
            app.world_mut()
                .run_system_cached(|mut events: RefreshEvents| events.requested())
                .unwrap()
        };
        assert!(!requested(&mut app));
        app.world_mut().write_message(WindowFocused {
            window: Entity::PLACEHOLDER,
            focused: false,
        });
        app.world_mut().write_message(AppLifecycle::WillSuspend);
        assert!(!requested(&mut app));
        app.world_mut().write_message(WindowFocused {
            window: Entity::PLACEHOLDER,
            focused: true,
        });
        assert!(requested(&mut app));
        for event in [AppLifecycle::WillResume, AppLifecycle::Running] {
            app.world_mut().write_message(event);
            assert!(requested(&mut app));
        }
    }
}
