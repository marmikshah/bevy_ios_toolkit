// Power-signal shim for bevy_ios_toolkit's `platform` feature.
//
// @_cdecl C-ABI called FROM Rust; polled getters (no callbacks). Behind
// #if canImport(UIKit) with linking stubs otherwise. Symbol prefix `platform_`.
//
// `AtomicInt` is declared in SafeArea.swift — same target, same purpose.

import Foundation

#if canImport(UIKit)

/// Cached so a per-frame poll from Rust never touches ProcessInfo. Seeded once
/// and refreshed only when the system says something changed.
private let thermalCache = AtomicInt(0)
private let lowPowerCache = AtomicInt(0)

private func refreshPower() {
    let info = ProcessInfo.processInfo
    thermalCache.value = info.thermalState.rawValue
    lowPowerCache.value = info.isLowPowerModeEnabled ? 1 : 0
}

/// Seeds the caches and subscribes to both change notifications. A global `let`
/// initializes exactly once, on first touch, under the runtime's own lock — so
/// the getters below can be called from any thread without a registration race.
private let powerObservers: Bool = {
    refreshPower()
    let center = NotificationCenter.default
    for name in [
        ProcessInfo.thermalStateDidChangeNotification,
        .NSProcessInfoPowerStateDidChange,
    ] {
        center.addObserver(forName: name, object: nil, queue: nil) { _ in refreshPower() }
    }
    return true
}()

/// 0 nominal, 1 fair, 2 serious, 3 critical — the `ProcessInfo.ThermalState`
/// raw values, which Rust mirrors.
@_cdecl("platform_thermal_state")
public func platform_thermal_state() -> Int32 {
    _ = powerObservers
    return Int32(thermalCache.value)
}

@_cdecl("platform_low_power_mode")
public func platform_low_power_mode() -> Int32 {
    _ = powerObservers
    return Int32(lowPowerCache.value)
}

#else
@_cdecl("platform_thermal_state") public func platform_thermal_state() -> Int32 { 0 }
@_cdecl("platform_low_power_mode") public func platform_low_power_mode() -> Int32 { 0 }
#endif
