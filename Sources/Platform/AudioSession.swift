// Audio-session shim for bevy_ios_toolkit's `platform` feature.
//
// @_cdecl C-ABI called FROM Rust; no callbacks. Behind #if canImport(UIKit)
// with a linking stub otherwise. Symbol prefix `platform_`.

import Foundation

#if canImport(UIKit)
import AVFoundation
import UIKit

// Retained so the foreground-reassert observer is installed at most once.
@MainActor private var audioSessionObserver: NSObjectProtocol?

/// Own the app's audio session so nothing else — a full-screen video ad
/// reconfiguring it, the OS default — can leave the game muted. `playback` != 0
/// keeps sound on with the ring switch off (audio as a game feature, gated by
/// an in-app toggle); otherwise `.ambient` respects the switch. `mixWithOthers`
/// != 0 lets the player's own music keep playing. Re-asserted on every
/// foreground return, which covers cold boot AND a full-screen ad dismissing.
@_cdecl("platform_configure_audio_session")
public func platform_configure_audio_session(_ playback: Int32, _ mixWithOthers: Int32) {
    let usePlayback = playback != 0
    let mix = mixWithOthers != 0
    DispatchQueue.main.async {
        applyAudioSession(playback: usePlayback, mix: mix)
        if audioSessionObserver == nil {
            audioSessionObserver = NotificationCenter.default.addObserver(
                forName: UIApplication.didBecomeActiveNotification,
                object: nil,
                queue: .main
            ) { _ in applyAudioSession(playback: usePlayback, mix: mix) }
        }
    }
}

private func applyAudioSession(playback: Bool, mix: Bool) {
    let session = AVAudioSession.sharedInstance()
    let category: AVAudioSession.Category = playback ? .playback : .ambient
    let options: AVAudioSession.CategoryOptions = mix ? [.mixWithOthers] : []
    do {
        try session.setCategory(category, options: options)
        try session.setActive(true)
    } catch {
        // Non-fatal: worst case the OS default session stands.
    }
}

#else
@_cdecl("platform_configure_audio_session") public func platform_configure_audio_session(_ playback: Int32, _ mixWithOthers: Int32) {}
#endif
