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

/// Top safe-area inset in points, cached so Rust can poll it synchronously from
/// any thread. The first call may return 0; callers re-ask after the window
/// exists.
private let safeTopCache = AtomicInt(0)

@_cdecl("platform_safe_top")
public func platform_safe_top() -> Float {
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
@_cdecl("platform_open_url") public func platform_open_url(_ url: UnsafePointer<CChar>) {}

#endif
