// First-frame boot-shield shim for bevy_ios_toolkit's `platform` feature.
//
// @_cdecl C-ABI called FROM Rust; no callbacks. Behind #if canImport(UIKit)
// with linking stubs otherwise. Symbol prefix `platform_`.

import Foundation

#if canImport(UIKit)
import UIKit

/// An opaque window held over the winit view from launch until the first real
/// frame, covering the uninitialized `CAMetalLayer` (the magenta/black flash).
@MainActor private var bootShieldWindow: UIWindow?

@_cdecl("platform_boot_shield_show")
public func platform_boot_shield_show(_ r: Float, _ g: Float, _ b: Float) {
    DispatchQueue.main.async {
        guard bootShieldWindow == nil else { return }
        let scene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive } ?? UIApplication.shared
            .connectedScenes.compactMap { $0 as? UIWindowScene }.first
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: UIScreen.main.bounds)
        // Above everything, non-interactive, so it never eats input meant for the
        // game and always sits over the Metal layer.
        window.windowLevel = .alert + 1
        window.isUserInteractionEnabled = false
        window.backgroundColor = UIColor(
            red: CGFloat(r), green: CGFloat(g), blue: CGFloat(b), alpha: 1.0)
        window.isHidden = false
        bootShieldWindow = window
    }
}

@_cdecl("platform_boot_shield_dismiss")
public func platform_boot_shield_dismiss() {
    DispatchQueue.main.async {
        guard let window = bootShieldWindow else { return }
        bootShieldWindow = nil
        UIView.animate(
            withDuration: 0.2,
            animations: { window.alpha = 0 },
            completion: { _ in window.isHidden = true })
    }
}

#else
@_cdecl("platform_boot_shield_show") public func platform_boot_shield_show(_ r: Float, _ g: Float, _ b: Float) {}
@_cdecl("platform_boot_shield_dismiss") public func platform_boot_shield_dismiss() {}
#endif
