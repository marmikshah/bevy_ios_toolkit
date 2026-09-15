import UIKit

// Exercise the real Objective-C selector replacement with the class name and
// launch notification used by winit 0.30. The demo covers the full Rust runner.
@objc(WinitUIWindow)
final class TestWinitWindow: UIWindow {}

@objc(TestAppDelegate)
final class TestAppDelegate: UIResponder, UIApplicationDelegate {}

@main
enum SceneSmoke {
    @MainActor static var window: UIWindow?
    @MainActor static var launchObserver: NSObjectProtocol?

    @MainActor static func main() {
        platform_register_scene_delegate()
        platform_register_scene_delegate() // registration must be idempotent
        precondition(NSClassFromString("BevyIosToolkitSceneDelegate") != nil)
        var startup = [Float](repeating: 0, count: 2)
        platform_screen_size(&startup)
        precondition(startup[0] == Float(UIScreen.main.bounds.width))
        precondition(startup[1] == Float(UIScreen.main.bounds.height))
        precondition(startup.allSatisfy { $0 > 0 })

        launchObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.didFinishLaunchingNotification, object: nil, queue: .main
        ) { _ in
            MainActor.assumeIsolated {
                let host = TestWinitWindow(frame: CGRect(x: 0, y: 0, width: 100, height: 100))
                let root = UIViewController()
                root.view.backgroundColor = .systemGreen
                host.rootViewController = root
                window = host
                host.makeKeyAndVisible()
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                    verify(host: host, root: root)
                }
            }
        }
        UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, "TestAppDelegate")
    }

    @MainActor static func verify(host: UIWindow, root: UIViewController) {
        guard let scene = host.windowScene else {
            preconditionFailure("winit window was not attached before presentation")
        }
        precondition(host.isKeyWindow && !host.isHidden)
        precondition(host.rootViewController === root)
        precondition(host.frame == scene.coordinateSpace.bounds)
        var size = [Float](repeating: 0, count: 2)
        platform_screen_size(&size)
        precondition(size == [Float(host.bounds.width), Float(host.bounds.height)])

        // A later window resize must replace the startup screen measurement.
        host.bounds.size = CGSize(width: 320, height: 480)
        platform_screen_size(&size)
        precondition(size == [320, 480])

        // Windows shown after willConnect must attach on their first show too.
        let later = TestWinitWindow(frame: .zero)
        later.rootViewController = UIViewController()
        later.makeKeyAndVisible()
        precondition(later.windowScene === scene)
        precondition(later.frame == scene.coordinateSpace.bounds)
        later.isHidden = true

        let ordinary = UIWindow(frame: .zero)
        ordinary.makeKeyAndVisible()
        precondition(ordinary.windowScene == nil, "unrelated window was changed")
        ordinary.isHidden = true
        host.makeKeyAndVisible() // repeat must call through without recursion
        precondition(host.windowScene === scene)

        // A worker must finish while the main thread waits, without a sync
        // dispatch deadlock. It receives the measurement cached above.
        let finished = DispatchSemaphore(value: 0)
        DispatchQueue.global().async {
            var cached = [Float](repeating: 0, count: 2)
            platform_screen_size(&cached)
            precondition(cached == [320, 480])
            finished.signal()
        }
        precondition(finished.wait(timeout: .now() + 2) == .success)
        print("SCENE_TESTS_PASSED: startup points, attachment, scope, resize, worker polling")
        exit(0)
    }
}
