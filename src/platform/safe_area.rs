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
mod tests;
