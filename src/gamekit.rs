//! Game Center (GameKit) as Bevy resources + messages: authentication,
//! leaderboard score submission, and achievement reporting.
//!
//! Flow:
//! 1. Send [`AuthenticateGameCenter`] once at startup. Read [`GameCenter`] —
//!    `auth` — for the result; submissions only succeed once `Authenticated`.
//! 2. Send [`SubmitScore`] / [`ReportAchievement`] as the player progresses.
//! 3. Send [`ShowGameCenter`] to present the native dashboard.
//!
//! Submissions are fire-and-forget; the Swift side logs failures. Authentication
//! is the one piece of polled state, because it gates the rest.
//!
//! # Desktop fake (non-iOS)
//!
//! [`AuthenticateGameCenter`] resolves to `Authenticated`; set
//! `BEVY_IOS_FAKE_GAMECENTER=unavailable` to simulate a signed-out device.
//! Submissions are recorded in-memory (see the tests).

use bevy::prelude::*;

#[cfg(target_os = "ios")]
mod backend {
    use std::ffi::c_char;
    unsafe extern "C" {
        pub fn gamekit_authenticate();
        /// 0 unknown, 1 authenticating, 2 authenticated, 3 unavailable.
        pub fn gamekit_auth_state() -> i32;
        pub fn gamekit_submit_score(leaderboard_id: *const c_char, score: i64);
        pub fn gamekit_report_achievement(achievement_id: *const c_char, percent: f64);
        pub fn gamekit_show_dashboard();
    }
}

#[cfg(not(target_os = "ios"))]
mod backend {
    use std::ffi::{CStr, c_char};
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::{LazyLock, Mutex};

    static STATE: AtomicI32 = AtomicI32::new(0);
    static SCORES: LazyLock<Mutex<Vec<(String, i64)>>> = LazyLock::new(|| Mutex::new(Vec::new()));
    static ACHIEVEMENTS: LazyLock<Mutex<Vec<(String, f64)>>> =
        LazyLock::new(|| Mutex::new(Vec::new()));

    unsafe fn cstr(ptr: *const c_char) -> String {
        if ptr.is_null() {
            return String::new();
        }
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }

    pub unsafe fn gamekit_authenticate() {
        let unavailable = std::env::var("BEVY_IOS_FAKE_GAMECENTER")
            .map(|v| v.eq_ignore_ascii_case("unavailable"))
            .unwrap_or(false);
        STATE.store(if unavailable { 3 } else { 2 }, Ordering::SeqCst);
    }

    pub unsafe fn gamekit_auth_state() -> i32 {
        STATE.load(Ordering::SeqCst)
    }

    pub unsafe fn gamekit_submit_score(leaderboard_id: *const c_char, score: i64) {
        let id = unsafe { cstr(leaderboard_id) };
        SCORES
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((id, score));
    }

    pub unsafe fn gamekit_report_achievement(achievement_id: *const c_char, percent: f64) {
        let id = unsafe { cstr(achievement_id) };
        ACHIEVEMENTS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((id, percent));
    }

    pub unsafe fn gamekit_show_dashboard() {}

    #[cfg(test)]
    pub fn reset() {
        STATE.store(0, Ordering::SeqCst);
        SCORES.lock().unwrap_or_else(|p| p.into_inner()).clear();
        ACHIEVEMENTS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }

    #[cfg(test)]
    pub fn submitted_scores() -> Vec<(String, i64)> {
        SCORES.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    #[cfg(test)]
    pub fn reported_achievements() -> Vec<(String, f64)> {
        ACHIEVEMENTS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

/// Game Center authentication state.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GameCenterAuthState {
    #[default]
    Unknown,
    Authenticating,
    Authenticated,
    /// Signed out, restricted, or Game Center disabled. Submissions are dropped.
    Unavailable,
}

impl GameCenterAuthState {
    fn from_i32(v: i32) -> GameCenterAuthState {
        match v {
            1 => GameCenterAuthState::Authenticating,
            2 => GameCenterAuthState::Authenticated,
            3 => GameCenterAuthState::Unavailable,
            _ => GameCenterAuthState::Unknown,
        }
    }
}

/// Game Center state. `auth` gates leaderboard/achievement submission.
#[derive(Resource, Default)]
pub struct GameCenter {
    pub auth: GameCenterAuthState,
}

impl GameCenter {
    pub fn is_authenticated(&self) -> bool {
        self.auth == GameCenterAuthState::Authenticated
    }
}

/// Authenticate the local player (presents the Game Center sign-in if needed).
#[derive(Message, Clone, Debug)]
pub struct AuthenticateGameCenter;

/// Submit `score` to the leaderboard with this id.
#[derive(Message, Clone, Debug)]
pub struct SubmitScore {
    pub leaderboard_id: String,
    pub score: i64,
}

/// Report achievement progress (`percent` in `0.0..=100.0`).
#[derive(Message, Clone, Debug)]
pub struct ReportAchievement {
    pub achievement_id: String,
    pub percent: f64,
}

/// Present the native Game Center dashboard.
#[derive(Message, Clone, Debug)]
pub struct ShowGameCenter;

/// Emitted when [`GameCenter::auth`] changes.
#[derive(Message, Clone, Debug)]
pub struct GameCenterAuthChanged(pub GameCenterAuthState);

pub struct GameKitPlugin;

impl Plugin for GameKitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameCenter>()
            .add_message::<AuthenticateGameCenter>()
            .add_message::<SubmitScore>()
            .add_message::<ReportAchievement>()
            .add_message::<ShowGameCenter>()
            .add_message::<GameCenterAuthChanged>()
            .add_systems(Update, (pump_requests, poll_auth).chain());
    }
}

fn pump_requests(
    gc: Res<GameCenter>,
    mut auths: MessageReader<AuthenticateGameCenter>,
    mut scores: MessageReader<SubmitScore>,
    mut achievements: MessageReader<ReportAchievement>,
    mut dashboards: MessageReader<ShowGameCenter>,
) {
    for _ in auths.read() {
        unsafe { backend::gamekit_authenticate() };
    }
    for s in scores.read() {
        if gc.is_authenticated()
            && let Ok(id) = std::ffi::CString::new(s.leaderboard_id.as_str())
        {
            unsafe { backend::gamekit_submit_score(id.as_ptr(), s.score) };
        }
    }
    for a in achievements.read() {
        if gc.is_authenticated()
            && let Ok(id) = std::ffi::CString::new(a.achievement_id.as_str())
        {
            unsafe {
                backend::gamekit_report_achievement(id.as_ptr(), a.percent.clamp(0.0, 100.0))
            };
        }
    }
    for _ in dashboards.read() {
        unsafe { backend::gamekit_show_dashboard() };
    }
}

fn poll_auth(mut gc: ResMut<GameCenter>, mut changed: MessageWriter<GameCenterAuthChanged>) {
    let current = GameCenterAuthState::from_i32(unsafe { backend::gamekit_auth_state() });
    if current != gc.auth {
        gc.auth = current;
        changed.write(GameCenterAuthChanged(current));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fake is a process-global singleton; serialize the tests that drive it.
    static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn guarded() -> std::sync::MutexGuard<'static, ()> {
        let g = GUARD.lock().unwrap_or_else(|p| p.into_inner());
        backend::reset();
        unsafe { std::env::remove_var("BEVY_IOS_FAKE_GAMECENTER") };
        g
    }

    fn build() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(GameKitPlugin);
        app
    }

    #[test]
    fn authenticate_then_submit_records() {
        let _guard = guarded();
        let mut app = build();

        app.world_mut()
            .resource_mut::<Messages<AuthenticateGameCenter>>()
            .write(AuthenticateGameCenter);
        app.update(); // pump auth
        app.update(); // poll -> Authenticated
        assert!(app.world().resource::<GameCenter>().is_authenticated());

        app.world_mut()
            .resource_mut::<Messages<SubmitScore>>()
            .write(SubmitScore {
                leaderboard_id: "lb.highscore".into(),
                score: 4200,
            });
        app.world_mut()
            .resource_mut::<Messages<ReportAchievement>>()
            .write(ReportAchievement {
                achievement_id: "ach.first_win".into(),
                percent: 100.0,
            });
        app.update();

        assert_eq!(
            backend::submitted_scores(),
            vec![("lb.highscore".to_string(), 4200)]
        );
        assert_eq!(
            backend::reported_achievements(),
            vec![("ach.first_win".to_string(), 100.0)]
        );
    }

    #[test]
    fn submit_dropped_when_unavailable() {
        let _guard = guarded();
        unsafe { std::env::set_var("BEVY_IOS_FAKE_GAMECENTER", "unavailable") };
        let mut app = build();

        app.world_mut()
            .resource_mut::<Messages<AuthenticateGameCenter>>()
            .write(AuthenticateGameCenter);
        app.update();
        app.update();
        assert_eq!(
            app.world().resource::<GameCenter>().auth,
            GameCenterAuthState::Unavailable
        );

        app.world_mut()
            .resource_mut::<Messages<SubmitScore>>()
            .write(SubmitScore {
                leaderboard_id: "lb.highscore".into(),
                score: 10,
            });
        app.update();
        assert!(
            backend::submitted_scores().is_empty(),
            "no submit while unauthenticated"
        );

        unsafe { std::env::remove_var("BEVY_IOS_FAKE_GAMECENTER") };
    }
}
