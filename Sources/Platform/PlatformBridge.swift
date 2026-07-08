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

/// Prepared impact generators; index = haptic kind from Rust.
/// 0 light, 1 medium, 2 heavy, 3 rigid (sharp tick).
@MainActor private var hapticGenerators: [UIImpactFeedbackGenerator] = []

@_cdecl("platform_haptic")
public func platform_haptic(_ kind: Int32) {
    DispatchQueue.main.async {
        if hapticGenerators.isEmpty {
            let styles: [UIImpactFeedbackGenerator.FeedbackStyle] = [.light, .medium, .heavy, .rigid]
            hapticGenerators = styles.map { UIImpactFeedbackGenerator(style: $0) }
            hapticGenerators.forEach { $0.prepare() }
        }
        let i = Int(kind)
        guard hapticGenerators.indices.contains(i) else { return }
        hapticGenerators[i].impactOccurred()
        hapticGenerators[i].prepare()
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

#else
// UIKit unavailable: linking stubs.

@_cdecl("platform_haptic") public func platform_haptic(_ kind: Int32) {}
@_cdecl("platform_safe_top") public func platform_safe_top() -> Float { 0 }
@_cdecl("platform_safe_bottom") public func platform_safe_bottom() -> Float { 0 }
@_cdecl("platform_safe_left") public func platform_safe_left() -> Float { 0 }
@_cdecl("platform_safe_right") public func platform_safe_right() -> Float { 0 }
@_cdecl("platform_open_url") public func platform_open_url(_ url: UnsafePointer<CChar>) {}

#endif
