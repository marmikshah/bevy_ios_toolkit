//! The device's safe-area insets (notch / Dynamic Island, home indicator,
//! rounded corners), in points.
//!
//! Insets are `0.0` when unavailable (off iOS, or before the key window exists);
//! the fallback policy is the caller's.
//!
//! Each [`insets`] call queues main-thread refreshes on iOS, so systems should
//! read the [`SafeArea`] resource instead.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_ios_toolkit::platform::safe_area::{Insets, SafeAreaEmulation, SafeAreaPlugin};
//!
//! App::new()
//!     .add_plugins((MinimalPlugins, SafeAreaPlugin))
//!     // A desktop capture laid out like a phone with a home indicator.
//!     .insert_resource(SafeAreaEmulation(Insets { top: 59.0, bottom: 34.0, ..default() }))
//!     .run();
//! ```

use std::time::Duration;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::{AppLifecycle, WindowCreated, WindowFocused, WindowResized};

/// The four safe-area insets, in points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl Insets {
    /// Parse `top,bottom,left,right` in points, such as `59,34` or
    /// `0,21,59,59`. Missing trailing edges are zero; each value must be a
    /// finite number of at least zero, and at most four may be given.
    /// Surrounding whitespace is ignored.
    pub fn parse(value: &str) -> Result<Self, String> {
        let mut edges = [0.0_f32; 4];
        let parts: Vec<&str> = value.split(',').map(str::trim).collect();
        if parts.len() > edges.len() {
            return Err("at most four insets: top,bottom,left,right".to_string());
        }
        for (edge, part) in edges.iter_mut().zip(parts) {
            *edge = part
                .parse::<f32>()
                .ok()
                .filter(|inset| inset.is_finite() && *inset >= 0.0)
                .ok_or_else(|| format!("invalid inset {part:?}: must be a non-negative number"))?;
        }
        let [top, bottom, left, right] = edges;
        Ok(Self {
            top,
            bottom,
            left,
            right,
        })
    }
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
/// Kept current by [`SafeAreaPlugin`], which
/// [`PlatformPlugin`](super::PlatformPlugin) adds, and written only when a
/// value changes, so `Res<SafeArea>::is_changed` marks real layout changes.
/// Insets stay zero off iOS and until UIKit installs the key window, unless
/// [`SafeAreaEmulation`] is present. Values are UIKit points, before any
/// `UiScale`.
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

/// Polls quickly after events that can change UIKit's geometry (the first
/// update can precede the key UIWindow), then slowly for UIKit-only inset
/// changes that produce no Bevy window event.
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
        // Read now unless a settling burst is already running.
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

/// Stand-in insets for builds without real ones, such as desktop store
/// captures laid out like a phone. While this resource exists,
/// [`SafeAreaPlugin`] reports these insets in place of the native reading on
/// every platform; [`SafeArea::size`] is still measured. Inserting, changing or
/// removing it takes effect on the next `PreUpdate`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct SafeAreaEmulation(pub Insets);

/// The `PreUpdate` system set that refreshes [`SafeArea`]. Order `PreUpdate`
/// readers `.after(RefreshSafeArea)` to see this frame's reading.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RefreshSafeArea;

/// Keeps the [`SafeArea`] resource current. [`PlatformPlugin`](super::PlatformPlugin)
/// adds it; add it directly to desktop or capture apps that read
/// `Res<SafeArea>` without the rest of the platform module. It needs no window
/// plugin and may be added more than once.
pub struct SafeAreaPlugin;

impl Plugin for SafeAreaPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<SafeAreaPolling>() {
            install(app);
        }
    }

    fn is_unique(&self) -> bool {
        false
    }
}

fn install(app: &mut App) {
    // Registered here too so the plugin works without `WindowPlugin`.
    app.add_message::<WindowCreated>()
        .add_message::<WindowResized>()
        .add_message::<WindowFocused>()
        .add_message::<AppLifecycle>()
        .init_resource::<SafeArea>()
        .init_resource::<SafeAreaSource>()
        .init_resource::<SafeAreaPolling>()
        .add_systems(PreUpdate, refresh.in_set(RefreshSafeArea));
}

fn refresh(
    time: Option<Res<Time<Real>>>,
    source: Res<SafeAreaSource>,
    emulation: Option<Res<SafeAreaEmulation>>,
    mut emulated: Local<bool>,
    mut polling: ResMut<SafeAreaPolling>,
    mut events: RefreshEvents,
    mut current: ResMut<SafeArea>,
) {
    // Inserting, changing or removing the emulation reads at once.
    let emulation_changed = emulation
        .as_ref()
        .is_some_and(|emulation| emulation.is_changed())
        || *emulated != emulation.is_some();
    *emulated = emulation.is_some();
    let requested = events.requested();
    // Without a clock there is no rate limit to keep: read every frame.
    if let Some(time) = time {
        let now = time.elapsed();
        if requested {
            polling.request(now);
        }
        if !polling.take_due(now) && !emulation_changed {
            return;
        }
    }
    let mut reading = (source.0)();
    if let Some(emulation) = emulation {
        reading.insets = emulation.0;
    }
    current.set_if_neq(reading);
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

    #[test]
    fn safe_area_plugin_works_alone_and_beside_the_platform_plugin() {
        for platform_first in [false, true] {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            if platform_first {
                app.add_plugins((crate::platform::PlatformPlugin, SafeAreaPlugin));
            } else {
                app.add_plugins((SafeAreaPlugin, crate::platform::PlatformPlugin));
            }
            app.update();
            assert_eq!(*app.world().resource::<SafeArea>(), SafeArea::default());
        }
        let mut app = App::new();
        app.add_plugins(SafeAreaPlugin);
        app.update();
        assert_eq!(*app.world().resource::<SafeArea>(), SafeArea::default());
    }

    #[test]
    fn emulated_insets_replace_the_reading_until_removed() {
        let phone = Insets {
            top: 59.0,
            bottom: 34.0,
            ..default()
        };
        let mut app = App::new();
        app.add_plugins(bevy::time::TimePlugin)
            .add_plugins(SafeAreaPlugin)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
            .insert_resource(SafeAreaEmulation(phone));
        app.update();
        assert_eq!(app.world().resource::<SafeArea>().insets, phone);
        // Changes apply at once, inside the poll's rate limit.
        let landscape = Insets {
            left: 59.0,
            right: 59.0,
            bottom: 21.0,
            ..default()
        };
        app.insert_resource(SafeAreaEmulation(landscape));
        app.update();
        assert_eq!(app.world().resource::<SafeArea>().insets, landscape);
        app.world_mut().remove_resource::<SafeAreaEmulation>();
        app.update();
        assert_eq!(*app.world().resource::<SafeArea>(), SafeArea::default());
    }

    #[test]
    fn insets_parse_top_bottom_left_right() {
        assert_eq!(
            Insets::parse("59,34"),
            Ok(Insets {
                top: 59.0,
                bottom: 34.0,
                ..default()
            })
        );
        assert_eq!(
            Insets::parse(" 0, 21 ,59,59.5 "),
            Ok(Insets {
                top: 0.0,
                bottom: 21.0,
                left: 59.0,
                right: 59.5,
            })
        );
        assert_eq!(Insets::parse("0"), Ok(Insets::default()));
        for bad in ["", "59,", "-1", "NaN", "inf", "a,b", "1,2,3,4,5"] {
            assert!(Insets::parse(bad).is_err(), "{bad}");
        }
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
