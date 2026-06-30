// Game Center (GameKit) bridge for bevy_ios_toolkit (feature "gamekit").
//
// Same contract as the other bridges: @_cdecl C-ABI called FROM Rust; the one
// piece of polled state is the authentication result (it gates submission).
// Score/achievement submission is fire-and-forget; failures are logged here.
// Add this file to the app's Xcode target when the `gamekit` feature is enabled,
// enable the Game Center capability, and create the leaderboards/achievements in
// App Store Connect. Deployment floor iOS 26 — the modern GameKit API needs no
// availability guards.

import Foundation
import os

#if canImport(GameKit)
import GameKit
#if canImport(UIKit)
import UIKit
#endif

final class GameKitBridge: NSObject, @unchecked Sendable {
    static let shared = GameKitBridge()

    // 0 unknown, 1 authenticating, 2 authenticated, 3 unavailable.
    private let authState = OSAllocatedUnfairLock(initialState: Int32(0))

    func authenticate() {
        authState.withLock { $0 = 1 }
        GKLocalPlayer.local.authenticateHandler = { [weak self] viewController, error in
            guard let self else { return }
            #if canImport(UIKit)
            if let viewController {
                self.present(viewController)
                return
            }
            #endif
            if GKLocalPlayer.local.isAuthenticated {
                self.authState.withLock { $0 = 2 }
            } else {
                if let error {
                    NSLog("[gamekit] authentication failed: %@", String(describing: error))
                }
                self.authState.withLock { $0 = 3 }
            }
        }
    }

    func authStateValue() -> Int32 { authState.withLock { $0 } }

    func submitScore(_ score: Int, leaderboardID: String) {
        guard GKLocalPlayer.local.isAuthenticated else { return }
        GKLeaderboard.submitScore(
            score, context: 0, player: GKLocalPlayer.local, leaderboardIDs: [leaderboardID]
        ) { error in
            if let error {
                NSLog("[gamekit] score submit failed: %@", String(describing: error))
            }
        }
    }

    func reportAchievement(_ id: String, percent: Double) {
        guard GKLocalPlayer.local.isAuthenticated else { return }
        let achievement = GKAchievement(identifier: id)
        achievement.percentComplete = percent
        achievement.showsCompletionBanner = true
        GKAchievement.report([achievement]) { error in
            if let error {
                NSLog("[gamekit] achievement report failed: %@", String(describing: error))
            }
        }
    }

    func showDashboard() {
        #if canImport(UIKit)
        DispatchQueue.main.async {
            let vc = GKGameCenterViewController(state: .dashboard)
            vc.gameCenterDelegate = self
            self.present(vc)
        }
        #endif
    }

    #if canImport(UIKit)
    private func present(_ viewController: UIViewController) {
        let root = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap { $0.windows }
            .first { $0.isKeyWindow }?
            .rootViewController
        root?.present(viewController, animated: true)
    }
    #endif
}

#if canImport(UIKit)
extension GameKitBridge: GKGameCenterControllerDelegate {
    func gameCenterViewControllerDidFinish(_ gameCenterViewController: GKGameCenterViewController) {
        gameCenterViewController.dismiss(animated: true)
    }
}
#endif

@_cdecl("gamekit_authenticate")
public func gamekit_authenticate() { GameKitBridge.shared.authenticate() }

@_cdecl("gamekit_auth_state")
public func gamekit_auth_state() -> Int32 { GameKitBridge.shared.authStateValue() }

@_cdecl("gamekit_submit_score")
public func gamekit_submit_score(_ leaderboardID: UnsafePointer<CChar>, _ score: Int64) {
    GameKitBridge.shared.submitScore(Int(score), leaderboardID: String(cString: leaderboardID))
}

@_cdecl("gamekit_report_achievement")
public func gamekit_report_achievement(_ achievementID: UnsafePointer<CChar>, _ percent: Double) {
    GameKitBridge.shared.reportAchievement(String(cString: achievementID), percent: percent)
}

@_cdecl("gamekit_show_dashboard")
public func gamekit_show_dashboard() { GameKitBridge.shared.showDashboard() }

#else
// GameKit unavailable: linking stubs. Reports unavailable, drops submissions.

@_cdecl("gamekit_authenticate") public func gamekit_authenticate() {}
@_cdecl("gamekit_auth_state") public func gamekit_auth_state() -> Int32 { 3 }
@_cdecl("gamekit_submit_score") public func gamekit_submit_score(_ leaderboardID: UnsafePointer<CChar>, _ score: Int64) {}
@_cdecl("gamekit_report_achievement") public func gamekit_report_achievement(_ achievementID: UnsafePointer<CChar>, _ percent: Double) {}
@_cdecl("gamekit_show_dashboard") public func gamekit_show_dashboard() {}

#endif
