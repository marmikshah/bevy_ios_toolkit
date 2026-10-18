// Outbound-links shim for bevy_ios_toolkit's `platform` feature.

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
