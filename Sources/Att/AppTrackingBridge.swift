// App Tracking Transparency bridge for bevy_ios_toolkit (feature "att").
//
// Link the `Att` product (the `Ads` product includes it). Requires
// `NSUserTrackingUsageDescription` in Info.plist.

import Foundation

#if canImport(AppTrackingTransparency) && canImport(UIKit)
import AppTrackingTransparency
import UIKit
import Platform

@MainActor
private let trackingRequest = TrackingRequestCoordinator(
    isDetermined: { ATTrackingManager.trackingAuthorizationStatus != .notDetermined },
    canPresent: { canPresentNativeSheet() && NativePromptCoordinator.shared.isIdle },
    prompt: {
        guard let lease = NativePromptCoordinator.shared.tryBegin(.tracking) else { return }
        defer { NativePromptCoordinator.shared.end(lease) }
        _ = await ATTrackingManager.requestTrackingAuthorization()
    }
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
