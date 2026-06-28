// Review-prompt bridge for bevy_ios_toolkit (feature "review").
//
// @_cdecl C-ABI called FROM Rust; fire-and-forget. Add this file to the app's
// Xcode target when the `review` feature is enabled. iOS decides whether to
// actually present the prompt (heavily rate-limited). Deployment floor iOS 26 —
// `AppStore.requestReview(in:)` (iOS 16+) needs no availability guard.

import Foundation

#if canImport(StoreKit) && canImport(UIKit)
import StoreKit
import UIKit

@_cdecl("review_request")
public func review_request() {
    DispatchQueue.main.async {
        let scene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
        guard let scene else { return }
        AppStore.requestReview(in: scene)
    }
}

#else
// Review unavailable: linking stub.

@_cdecl("review_request") public func review_request() {}

#endif
