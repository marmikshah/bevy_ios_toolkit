// App Tracking Transparency bridge for bevy_ios_toolkit (feature "att").
//
// Same contract as the other bridges: @_cdecl C-ABI called FROM Rust, polled
// status (no callbacks into Rust). Add this file to the app's Xcode target when
// the `att` feature is enabled. Requires `NSUserTrackingUsageDescription` in
// Info.plist. Deployment floor iOS 26 — ATT (iOS 14+) needs no availability guard.

import Foundation

#if canImport(AppTrackingTransparency) && canImport(UIKit)
import AppTrackingTransparency
import UIKit

@MainActor
private let trackingRequest = TrackingRequestCoordinator(
    isDetermined: { ATTrackingManager.trackingAuthorizationStatus != .notDetermined },
    canPresent: {
        UIApplication.shared.applicationState == .active
            && UIApplication.shared.connectedScenes
                .compactMap { $0 as? UIWindowScene }
                .filter { $0.activationState == .foregroundActive }
                .flatMap { $0.windows }
                .contains { $0.isKeyWindow && $0.rootViewController != nil
                    && $0.rootViewController?.presentedViewController == nil }
    },
    prompt: { _ = await ATTrackingManager.requestTrackingAuthorization() }
)

@_cdecl("att_request")
public func att_request() {
    // Bevy may call from a worker. UIKit work and duplicate requests are
    // serialized on the main actor; an interrupted sheet remains pending.
    Task { @MainActor in await trackingRequest.resolve() }
}

@_cdecl("att_status")
public func att_status() -> Int32 {
    switch ATTrackingManager.trackingAuthorizationStatus {
    case .notDetermined: return 0
    case .restricted: return 1
    case .denied: return 2
    case .authorized: return 3
    @unknown default: return 0
    }
}

#else
// ATT unavailable: linking stubs. Status reports not-determined.

@_cdecl("att_request") public func att_request() {}
@_cdecl("att_status") public func att_status() -> Int32 { 0 }

#endif
