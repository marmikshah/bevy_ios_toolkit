// Outbound-links shim for bevy_ios_toolkit's `platform` feature.
//
// @_cdecl C-ABI called FROM Rust; no callbacks. Behind #if canImport(UIKit)
// with a linking stub otherwise. Symbol prefix `platform_`.

import Foundation

#if canImport(UIKit)
import UIKit

@_cdecl("platform_open_url")
public func platform_open_url(_ url: UnsafePointer<CChar>) {
    let s = String(cString: url)
    DispatchQueue.main.async {
        guard let u = URL(string: s) else { return }
        UIApplication.shared.open(u)
    }
}

#else
@_cdecl("platform_open_url") public func platform_open_url(_ url: UnsafePointer<CChar>) {}
#endif
