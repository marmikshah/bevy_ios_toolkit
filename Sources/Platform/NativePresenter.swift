import Foundation

#if canImport(UIKit)
import UIKit

@MainActor
public func canPresentNativeSheet() -> Bool {
    guard UIApplication.shared.applicationState == .active else { return false }
    return UIApplication.shared.connectedScenes
        .compactMap { $0 as? UIWindowScene }
        .filter { $0.activationState == .foregroundActive }
        .flatMap { $0.windows }
        .contains { window in
            guard window.isKeyWindow, let controller = window.rootViewController else { return false }
            return controller.viewIfLoaded?.window != nil
                && controller.presentedViewController == nil
                && !controller.isBeingPresented && !controller.isBeingDismissed
        }
}
#else
@MainActor
public func canPresentNativeSheet() -> Bool { false }
#endif
