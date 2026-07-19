// Share-sheet shim for bevy_ios_toolkit's `platform` feature.
//
// @_cdecl C-ABI called FROM Rust; fire-and-forget (no callbacks, no polled
// result). Behind #if canImport(UIKit) with a linking stub otherwise. Symbol
// prefix `platform_`.

import Foundation

#if canImport(UIKit)
import UIKit

/// The view controller actually on screen. Presenting from the window's root
/// while a modal is up silently does nothing, so walk the presentation chain.
@MainActor private func topmostViewController() -> UIViewController? {
    var top = UIApplication.shared.connectedScenes
        .compactMap { $0 as? UIWindowScene }
        .flatMap { $0.windows }
        .first { $0.isKeyWindow }?.rootViewController
    while let presented = top?.presentedViewController {
        top = presented
    }
    return top
}

@_cdecl("platform_share_text")
public func platform_share_text(_ text: UnsafePointer<CChar>) {
    let s = String(cString: text)
    DispatchQueue.main.async {
        guard let top = topmostViewController() else { return }
        // A second sheet over the first is a no-op on iOS; drop it rather than
        // queue it, so a double-tap can't strand a sheet.
        guard !(top is UIActivityViewController) else { return }

        let vc = UIActivityViewController(activityItems: [s], applicationActivities: nil)
        // iPad presents this as a popover and traps on a nil source view; anchor
        // it to the centre of the presenting view, which reads as a modal sheet.
        if let popover = vc.popoverPresentationController {
            popover.sourceView = top.view
            popover.sourceRect = CGRect(
                x: top.view.bounds.midX, y: top.view.bounds.midY, width: 0, height: 0
            )
            popover.permittedArrowDirections = []
        }
        top.present(vc, animated: true)
    }
}

#else
@_cdecl("platform_share_text") public func platform_share_text(_ text: UnsafePointer<CChar>) {}
#endif
