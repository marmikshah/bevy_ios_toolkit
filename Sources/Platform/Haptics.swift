// Haptics shim for bevy_ios_toolkit's `platform` feature.
//
// @_cdecl C-ABI called FROM Rust; no callbacks back. Behind #if canImport(UIKit)
// with a linking stub otherwise, so the staticlib links on any target. Symbol
// prefix `platform_`.

import Foundation

#if canImport(UIKit)
import UIKit

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

#else
@_cdecl("platform_haptic") public func platform_haptic(_ kind: Int32) {}
#endif
