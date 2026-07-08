//! Own the app's audio session so nothing else — a full-screen video ad
//! reconfiguring it, or the OS default — can leave the game muted. iOS routes
//! through the C-ABI shim; other platforms no-op.
//!
//! Call [`configure`] once at startup on any app that turns on ads: the ad SDK
//! reconfigures the shared session when a video creative plays and can leave it
//! deactivated on dismiss, silencing the game's own audio. Owning the session
//! (and re-asserting it on every foreground return, which the shim does) keeps
//! the mix alive across ads.

/// Claim the app's audio session, re-asserted on every foreground return.
///
/// `playback` keeps sound on with the ring/silent switch off (audio as a game
/// feature, gated by an in-app toggle); `false` uses `.ambient`, which respects
/// the switch. `mix_with_others` lets the player's own music keep playing.
#[cfg(target_os = "ios")]
pub fn configure(playback: bool, mix_with_others: bool) {
    unsafe extern "C" {
        fn platform_configure_audio_session(playback: i32, mix_with_others: i32);
    }
    unsafe { platform_configure_audio_session(playback as i32, mix_with_others as i32) };
}

#[cfg(not(target_os = "ios"))]
pub fn configure(_playback: bool, _mix_with_others: bool) {}
