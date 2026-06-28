// Platform shim for bevy_cupertino: haptics, safe-area inset, outbound links.
//
// Same contract as StoreKitBridge.swift / NativeBridge.swift: @_cdecl C-ABI
// called FROM Rust, polled getters (no callbacks into Rust). Everything that
// needs UIKit sits behind #if canImport(UIKit) with linking stubs otherwise, so
// the staticlib links on any target. Symbol prefix `cupertino_`.
//
// Add this file to the app's Xcode target alongside StoreKitBridge.swift.

import Foundation

// Tiny lock-wrapped int — Foundation has no public atomic Int; uncontended
// (one writer per value). Mirrors NativeBridge.swift's ManagedAtomic.
final class CupertinoAtomicInt {
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

@_cdecl("cupertino_haptic")
public func cupertino_haptic(_ kind: Int32) {
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

/// Top safe-area inset in points, cached so Rust can poll it synchronously from
/// any thread. The first call may return 0; callers re-ask after the window
/// exists.
private let safeTopCache = CupertinoAtomicInt(0)

@_cdecl("cupertino_safe_top")
public func cupertino_safe_top() -> Float {
    DispatchQueue.main.async {
        let inset = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap { $0.windows }
            .first { $0.isKeyWindow }?.safeAreaInsets.top ?? 0
        safeTopCache.value = Int(inset.rounded())
    }
    return Float(safeTopCache.value)
}

// MARK: - Outbound links

@_cdecl("cupertino_open_url")
public func cupertino_open_url(_ url: UnsafePointer<CChar>) {
    let s = String(cString: url)
    DispatchQueue.main.async {
        guard let u = URL(string: s) else { return }
        UIApplication.shared.open(u)
    }
}

#else
// UIKit unavailable: linking stubs.

@_cdecl("cupertino_haptic") public func cupertino_haptic(_ kind: Int32) {}
@_cdecl("cupertino_safe_top") public func cupertino_safe_top() -> Float { 0 }
@_cdecl("cupertino_open_url") public func cupertino_open_url(_ url: UnsafePointer<CChar>) {}

#endif
