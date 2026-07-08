// Platform shim for bevy_ios_toolkit: haptics, safe-area inset, outbound links.
//
// Same contract as StoreKitBridge.swift: @_cdecl C-ABI called FROM Rust, polled
// getters (no callbacks into Rust). Everything that needs UIKit sits behind
// #if canImport(UIKit) with linking stubs otherwise, so the staticlib links on
// any target. Symbol prefix `platform_`.
//
// Shipped as this package's `Platform` product; link it from your app target.

import Foundation

// Tiny lock-wrapped int — Foundation has no public atomic Int; uncontended
// (one writer per value) — the same atomic discipline the other shims use.
final class AtomicInt {
    private let lock = NSLock()
    private var raw: Int
    init(_ v: Int) { raw = v }
    var value: Int {
        get { lock.lock(); defer { lock.unlock() }; return raw }
        set { lock.lock(); raw = newValue; lock.unlock() }
    }
}

#if canImport(UIKit)
import UIKit

// MARK: - Haptics

/// Prepared generators, built lazily and reused. Kind index from Rust:
/// 0–3 impact (light/medium/heavy/rigid), 4–6 notification
/// (success/warning/error), 7 selection.
@MainActor private var impactGenerators: [UIImpactFeedbackGenerator] = []
@MainActor private var notificationGenerator: UINotificationFeedbackGenerator?
@MainActor private var selectionGenerator: UISelectionFeedbackGenerator?

@_cdecl("platform_haptic")
public func platform_haptic(_ kind: Int32) {
    DispatchQueue.main.async {
        switch kind {
        case 0...3:
            if impactGenerators.isEmpty {
                let styles: [UIImpactFeedbackGenerator.FeedbackStyle] = [.light, .medium, .heavy, .rigid]
                impactGenerators = styles.map { UIImpactFeedbackGenerator(style: $0) }
                impactGenerators.forEach { $0.prepare() }
            }
            let g = impactGenerators[Int(kind)]
            g.impactOccurred()
            g.prepare()
        case 4...6:
            let types: [UINotificationFeedbackGenerator.FeedbackType] = [.success, .warning, .error]
            let g = notificationGenerator ?? UINotificationFeedbackGenerator()
            notificationGenerator = g
            g.notificationOccurred(types[Int(kind) - 4])
            g.prepare()
        case 7:
            let g = selectionGenerator ?? UISelectionFeedbackGenerator()
            selectionGenerator = g
            g.selectionChanged()
            g.prepare()
        default:
            return
        }
    }
}

// MARK: - Safe area

/// Safe-area insets in points, cached so Rust can poll each edge synchronously
/// from any thread. The first call may return 0; callers re-ask after the window
/// exists.
private let safeTopCache = AtomicInt(0)
private let safeBottomCache = AtomicInt(0)
private let safeLeftCache = AtomicInt(0)
private let safeRightCache = AtomicInt(0)

/// The key window's insets, or zeros before one exists.
@MainActor private func keyWindowInsets() -> UIEdgeInsets {
    UIApplication.shared.connectedScenes
        .compactMap { $0 as? UIWindowScene }
        .flatMap { $0.windows }
        .first { $0.isKeyWindow }?.safeAreaInsets ?? .zero
}

@_cdecl("platform_safe_top")
public func platform_safe_top() -> Float {
    DispatchQueue.main.async { safeTopCache.value = Int(keyWindowInsets().top.rounded()) }
    return Float(safeTopCache.value)
}

@_cdecl("platform_safe_bottom")
public func platform_safe_bottom() -> Float {
    DispatchQueue.main.async { safeBottomCache.value = Int(keyWindowInsets().bottom.rounded()) }
    return Float(safeBottomCache.value)
}

@_cdecl("platform_safe_left")
public func platform_safe_left() -> Float {
    DispatchQueue.main.async { safeLeftCache.value = Int(keyWindowInsets().left.rounded()) }
    return Float(safeLeftCache.value)
}

@_cdecl("platform_safe_right")
public func platform_safe_right() -> Float {
    DispatchQueue.main.async { safeRightCache.value = Int(keyWindowInsets().right.rounded()) }
    return Float(safeRightCache.value)
}

// MARK: - Outbound links

@_cdecl("platform_open_url")
public func platform_open_url(_ url: UnsafePointer<CChar>) {
    let s = String(cString: url)
    DispatchQueue.main.async {
        guard let u = URL(string: s) else { return }
        UIApplication.shared.open(u)
    }
}

// MARK: - Boot shield

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
// UIKit unavailable: linking stubs.

@_cdecl("platform_haptic") public func platform_haptic(_ kind: Int32) {}
@_cdecl("platform_safe_top") public func platform_safe_top() -> Float { 0 }
@_cdecl("platform_safe_bottom") public func platform_safe_bottom() -> Float { 0 }
@_cdecl("platform_safe_left") public func platform_safe_left() -> Float { 0 }
@_cdecl("platform_safe_right") public func platform_safe_right() -> Float { 0 }
@_cdecl("platform_open_url") public func platform_open_url(_ url: UnsafePointer<CChar>) {}
@_cdecl("platform_boot_shield_show") public func platform_boot_shield_show(_ r: Float, _ g: Float, _ b: Float) {}
@_cdecl("platform_boot_shield_dismiss") public func platform_boot_shield_dismiss() {}

#endif
