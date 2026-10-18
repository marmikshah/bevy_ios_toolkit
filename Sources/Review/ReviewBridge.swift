// Review-prompt bridge for bevy_ios_toolkit (feature "review").
//
// Fire-and-forget; link the `Review` product. iOS decides whether to present
// the prompt (heavily rate-limited).

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
