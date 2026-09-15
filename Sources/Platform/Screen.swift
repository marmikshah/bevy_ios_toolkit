// Screen/window dimensions for startup window sizing. Points, never pixels.
import Foundation

#if canImport(UIKit)
import UIKit
import os

private let screenSizeCache = OSAllocatedUnfairLock(initialState: CGSize.zero)

@MainActor private func currentScreenSize() -> CGSize {
    let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
    let active = scenes.filter { $0.activationState == .foregroundActive }
    if let window = (active + scenes).flatMap({ $0.windows }).first(where: { $0.isKeyWindow }) {
        return window.bounds.size
    }
    if let scene = active.first {
        return scene.coordinateSpace.bounds.size
    }
    // Startup has neither a window nor a scene. UIScreen.main is deprecated
    // for running apps, but remains the bootstrap measurement in this case.
    return UIScreen.main.bounds.size
}

@MainActor private func refreshScreenSize() -> CGSize {
    let size = currentScreenSize()
    screenSizeCache.withLock { $0 = size }
    return size
}

@_cdecl("platform_screen_size")
public func platform_screen_size(_ out: UnsafeMutablePointer<Float>) {
    let size: CGSize
    if Thread.isMainThread {
        size = MainActor.assumeIsolated { refreshScreenSize() }
    } else {
        // A Bevy main thread can be waiting for its workers; sync dispatch
        // back to it would deadlock the frame.
        DispatchQueue.main.async { _ = refreshScreenSize() }
        size = screenSizeCache.withLock { $0 }
    }
    out[0] = Float(size.width)
    out[1] = Float(size.height)
}
#else
@_cdecl("platform_screen_size")
public func platform_screen_size(_ out: UnsafeMutablePointer<Float>) {
    out[0] = 0
    out[1] = 0
}
#endif
