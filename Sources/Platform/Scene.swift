// Temporary scene adoption for winit 0.30. Delete the method replacement when
// the supported winit version attaches its own windows to a UIWindowScene.
import Foundation

#if canImport(UIKit)
import UIKit
import ObjectiveC

@MainActor private enum SceneConnection {
    static weak var scene: UIWindowScene?
    static var installed = false

    static func install() {
        guard !installed else { return }
        guard let original = class_getInstanceMethod(UIWindow.self, #selector(UIWindow.makeKeyAndVisible)),
              let replacement = class_getInstanceMethod(UIWindow.self, #selector(UIWindow.bevyToolkit_makeKeyAndVisible))
        else { preconditionFailure("UIWindow.makeKeyAndVisible is unavailable") }
        method_exchangeImplementations(original, replacement)
        installed = true
    }
}

extension UIWindow {
    @objc fileprivate dynamic func bevyToolkit_makeKeyAndVisible() {
        // Ad SDKs, keyboards, boot shields, and scene-aware windows keep their
        // original behavior. This workaround owns only winit 0.30's orphan.
        if windowScene == nil,
           let winitClass = NSClassFromString("WinitUIWindow"), isKind(of: winitClass),
           let scene = SceneConnection.scene {
            windowScene = scene
            frame = scene.coordinateSpace.bounds
        }
        // After the exchange this selector invokes UIKit's original method.
        bevyToolkit_makeKeyAndVisible()
    }
}

/// Name this class verbatim in UIApplicationSceneManifest. The app keeps
/// winit's UIApplication entry point and needs no Swift delegate of its own.
/// A single application scene is supported; disable multiple scene sessions.
@objc(BevyIosToolkitSceneDelegate)
public final class BevyIosToolkitSceneDelegate: NSObject, UIWindowSceneDelegate {
    public func scene(_ scene: UIScene, willConnectTo session: UISceneSession,
                      options connectionOptions: UIScene.ConnectionOptions) {
        guard let scene = scene as? UIWindowScene,
              session.role == .windowApplication else { return }
        SceneConnection.scene = scene
        SceneConnection.install()
    }

    public func sceneDidDisconnect(_ scene: UIScene) {
        if SceneConnection.scene === scene { SceneConnection.scene = nil }
    }
    // UIKit already sends the app-level notifications winit observes. Do not
    // re-post them from scene callbacks: that duplicates lifecycle events.
}

@_cdecl("platform_register_scene_delegate")
public func platform_register_scene_delegate() {
    // A Rust reference to this C symbol retains the delegate in static builds
    // before UIApplicationMain resolves its name from Info.plist.
    _ = NSStringFromClass(BevyIosToolkitSceneDelegate.self)
}
#else
@_cdecl("platform_register_scene_delegate")
public func platform_register_scene_delegate() {}
#endif
