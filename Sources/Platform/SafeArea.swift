// Safe-area shim for bevy_ios_toolkit's `platform` feature.
//
// @_cdecl C-ABI called FROM Rust; polled getters (no callbacks). Behind
// #if canImport(UIKit) with linking stubs otherwise. Symbol prefix `platform_`.

import Foundation

// Tiny lock-wrapped int — Foundation has no public atomic Int; uncontended
// (one writer per value). Always compiled: the getters cache through it.
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

#else
@_cdecl("platform_safe_top") public func platform_safe_top() -> Float { 0 }
@_cdecl("platform_safe_bottom") public func platform_safe_bottom() -> Float { 0 }
@_cdecl("platform_safe_left") public func platform_safe_left() -> Float { 0 }
@_cdecl("platform_safe_right") public func platform_safe_right() -> Float { 0 }
#endif
