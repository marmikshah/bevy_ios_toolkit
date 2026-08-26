// Local user notifications bridge for bevy_ios_toolkit (feature "notifications").
//
// Design contract with the Rust side (the bevy_ios_toolkit notifications module):
//   - Every entry point is @_cdecl C-ABI, called FROM Rust.
//   - UNUserNotificationCenter's async work surfaces as CACHED STATE and a
//     POLLED EVENT QUEUE drained once per frame, never as callbacks into Rust —
//     re-entrancy against winit's event loop is not safe. This mirrors
//     AdMobBridge.swift and StoreKitBridge.swift.
//   - notifications_status never blocks. It answers from cache and kicks off a
//     refresh. Asking the system synchronously would mean parking a Bevy worker
//     thread on the main thread, which deadlocks with no crash and no log.
//   - notifications_drain_events returns a pointer to a buffer owned here, valid
//     only until the next drain; Rust copies immediately.
//   - This file must COMPILE AND LINK where the framework is unavailable:
//     everything UserNotifications-specific sits behind
//     #if canImport(UserNotifications), with linking stubs otherwise.
//
// LOCAL ONLY. No APNs, no device token, no server, and nothing here that counts
// as data collection on a privacy manifest. No Info.plist key is required.
//
// Integration (per game):
//   1. Link this package's `Notifications` product from your app target.
//   2. To catch a notification that LAUNCHED the app, call
//      notifications_install_delegate() from your app delegate's
//      didFinishLaunchingWithOptions. iOS delivers that response very early, and
//      a response delivered with no delegate set is gone. Installing from Rust's
//      first frame catches everything after that, and is idempotent.
//
// Shared state (the event queue + cached authorization) lives behind an
// OSAllocatedUnfairLock so async system callbacks and the synchronous C-ABI
// getters stay race-free and Swift-6 clean.

import Foundation
import os

#if canImport(UserNotifications)
import UserNotifications

private struct NotificationsSharedState {
    /// 0 not-determined, 1 denied, 2 authorized, 3 provisional.
    var status: Int32 = 0
    var events: [[String: Any]] = []
    var eventsJSONPtr: UnsafeMutablePointer<CChar>?
    var delegateInstalled = false
    var refreshing = false
}

final class NotificationsBridge: NSObject, @unchecked Sendable {
    static let shared = NotificationsBridge()

    private let shared_ = OSAllocatedUnfairLock(initialState: NotificationsSharedState())

    // MARK: State

    func statusValue() -> Int32 {
        // Answer from cache, then refresh behind the caller's back. One refresh
        // in flight at a time: this is called every frame.
        let shouldRefresh = shared_.withLock { state -> Bool in
            if state.refreshing { return false }
            state.refreshing = true
            return true
        }
        if shouldRefresh {
            UNUserNotificationCenter.current().getNotificationSettings { [weak self] settings in
                self?.shared_.withLock { state in
                    state.status = Self.code(for: settings.authorizationStatus)
                    state.refreshing = false
                }
            }
        }
        return shared_.withLock { $0.status }
    }

    private static func code(for status: UNAuthorizationStatus) -> Int32 {
        switch status {
        case .notDetermined: return 0
        case .denied: return 1
        case .authorized: return 2
        // Ephemeral is App Clip lifetime; like provisional it delivers quietly,
        // which is the only distinction the Rust side draws.
        case .provisional, .ephemeral: return 3
        @unknown default: return 0
        }
    }

    private func push(_ event: [String: Any]) {
        shared_.withLock { $0.events.append(event) }
    }

    func drainEvents() -> UnsafePointer<CChar>? {
        shared_.withLockUnchecked { state in
            let json = (try? JSONSerialization.data(withJSONObject: state.events))
                .flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
            state.events.removeAll(keepingCapacity: true)
            if let old = state.eventsJSONPtr { free(old) }
            state.eventsJSONPtr = strdup(json)
            return state.eventsJSONPtr.map { UnsafePointer($0) }
        }
    }

    // MARK: Entry points

    func installDelegate() {
        let alreadyInstalled = shared_.withLock { state -> Bool in
            defer { state.delegateInstalled = true }
            return state.delegateInstalled
        }
        guard !alreadyInstalled else { return }
        // Setting the delegate touches UIKit-adjacent state; keep it on main.
        DispatchQueue.main.async {
            UNUserNotificationCenter.current().delegate = self
        }
    }

    func request() {
        UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound, .badge]) { [weak self] _, _ in
                // Ignore the granted flag and read the settings back: it is the
                // authoritative answer, and it also covers provisional.
                self?.refreshNow()
            }
    }

    private func refreshNow() {
        UNUserNotificationCenter.current().getNotificationSettings { [weak self] settings in
            self?.shared_.withLock { $0.status = Self.code(for: settings.authorizationStatus) }
        }
    }

    func schedule(id: String, title: String, body: String, after: Double) -> Int32 {
        // The refusals the Rust side names, made here for the same reasons.
        guard !id.isEmpty, after > 0 else { return 1 }
        let status = shared_.withLock { $0.status }
        guard status == 2 || status == 3 else { return 2 }

        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default
        let trigger = UNTimeIntervalNotificationTrigger(timeInterval: after, repeats: false)
        let request = UNNotificationRequest(identifier: id, content: content, trigger: trigger)
        // Adding with an existing identifier replaces it, which is the contract
        // the Rust side documents: a slipping timer must not queue two.
        UNUserNotificationCenter.current().add(request) { [weak self] error in
            guard let error else { return }
            self?.push([
                "kind": "schedule_failed",
                "id": id,
                "reason": error.localizedDescription,
            ])
        }
        return 0
    }

    func cancel(id: String) {
        let center = UNUserNotificationCenter.current()
        center.removePendingNotificationRequests(withIdentifiers: [id])
        // A delivered one still sitting in the notification centre is stale the
        // moment the app cancels it.
        center.removeDeliveredNotifications(withIdentifiers: [id])
    }

    func cancelAll() {
        let center = UNUserNotificationCenter.current()
        center.removeAllPendingNotificationRequests()
        center.removeAllDeliveredNotifications()
    }
}

extension NotificationsBridge: UNUserNotificationCenterDelegate {
    func userNotificationCenter(
        _: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        push(["kind": "opened", "id": response.notification.request.identifier])
        completionHandler()
    }

    /// Show it even while the app is in the foreground. Without this the system
    /// silently suppresses it, which reads as a broken schedule.
    func userNotificationCenter(
        _: UNUserNotificationCenter,
        willPresent _: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound])
    }
}

@_cdecl("notifications_install_delegate")
public func notifications_install_delegate() {
    NotificationsBridge.shared.installDelegate()
}

@_cdecl("notifications_request")
public func notifications_request() {
    NotificationsBridge.shared.request()
}

@_cdecl("notifications_status")
public func notifications_status() -> Int32 {
    NotificationsBridge.shared.statusValue()
}

@_cdecl("notifications_schedule")
public func notifications_schedule(
    _ id: UnsafePointer<CChar>,
    _ title: UnsafePointer<CChar>,
    _ body: UnsafePointer<CChar>,
    _ afterSecs: Double
) -> Int32 {
    NotificationsBridge.shared.schedule(
        id: String(cString: id),
        title: String(cString: title),
        body: String(cString: body),
        after: afterSecs
    )
}

@_cdecl("notifications_cancel")
public func notifications_cancel(_ id: UnsafePointer<CChar>) {
    NotificationsBridge.shared.cancel(id: String(cString: id))
}

@_cdecl("notifications_cancel_all")
public func notifications_cancel_all() {
    NotificationsBridge.shared.cancelAll()
}

@_cdecl("notifications_drain_events")
public func notifications_drain_events() -> UnsafePointer<CChar>? {
    NotificationsBridge.shared.drainEvents()
}

#else
// UserNotifications unavailable: linking stubs. Status reports not-determined,
// scheduling is refused, and nothing is ever delivered.

@_cdecl("notifications_install_delegate") public func notifications_install_delegate() {}
@_cdecl("notifications_request") public func notifications_request() {}
@_cdecl("notifications_status") public func notifications_status() -> Int32 { 0 }

@_cdecl("notifications_schedule")
public func notifications_schedule(
    _ id: UnsafePointer<CChar>,
    _ title: UnsafePointer<CChar>,
    _ body: UnsafePointer<CChar>,
    _ afterSecs: Double
) -> Int32 { 2 }

@_cdecl("notifications_cancel") public func notifications_cancel(_ id: UnsafePointer<CChar>) {}
@_cdecl("notifications_cancel_all") public func notifications_cancel_all() {}

@_cdecl("notifications_drain_events")
public func notifications_drain_events() -> UnsafePointer<CChar>? { nil }

#endif
