// StoreKit 2 bridge for bevy_ios_toolkit.
//
// Design contract with the Rust side (the bevy_ios_toolkit StoreKit module):
//   - Every entry point is @_cdecl C-ABI, called FROM Rust.
//   - StoreKit's async work runs in Tasks; results surface as POLLED state
//     (Int32 / owned C-string snapshots), never callbacks into Rust.
//   - Every returned string is an independent allocation. Rust releases it
//     through `store_string_free`, so async work cannot invalidate a read.
//   - This file must COMPILE AND LINK even where StoreKit is unavailable:
//     everything StoreKit-specific sits behind #if canImport(StoreKit), with
//     linking stubs otherwise.
//
// Integration: link this package's `Store` product from your app target; it
// links StoreKit and UIKit (for foreground reconciliation) for you.
// Symbol prefix `store_` avoids collision with the game's own bridge.

import Foundation
import os

#if canImport(StoreKit)
import StoreKit
#if canImport(UIKit)
import UIKit
#endif

enum EntitlementReadResult: Sendable {
    case ready(Set<String>)
    case failed(String)
}

enum EntitlementStatus: String, Sendable {
    case checking
    case ready
    case failed
}

struct EntitlementSnapshotState {
    private(set) var status = EntitlementStatus.checking
    private(set) var owned = Set<String>()
    private(set) var revision: UInt64 = 0

    mutating func publishReady(_ snapshot: Set<String>) -> Bool {
        let changed = status != .ready || owned != snapshot
        status = .ready
        owned = snapshot
        if changed { revision &+= 1 }
        return changed
    }

    mutating func publishFailed() -> Bool {
        let changed = status != .failed
        status = .failed
        if changed { revision &+= 1 }
        return changed
    }
}

/// One owner for every entitlement trigger. Calls arriving during a read are
/// queued behind it, so an older snapshot can never overwrite a newer one.
actor EntitlementReconciler {
    typealias Reader = @Sendable () async -> EntitlementReadResult
    typealias Publisher = @Sendable (EntitlementReadResult) -> Bool

    private struct Waiter {
        let request: UInt64
        let continuation: CheckedContinuation<Bool, Never>
    }

    private let read: Reader
    private let publish: Publisher
    private var requested: UInt64 = 0
    private var completed: UInt64 = 0
    private var isRunning = false
    private var waiters: [Waiter] = []

    init(read: @escaping Reader, publish: @escaping Publisher) {
        self.read = read
        self.publish = publish
    }

    func reconcile() async -> Bool {
        requested &+= 1
        let request = requested
        return await withCheckedContinuation { continuation in
            waiters.append(Waiter(request: request, continuation: continuation))
            guard !isRunning else { return }
            isRunning = true
            Task { await self.run() }
        }
    }

    private func run() async {
        while completed < requested {
            // Coalesce everything already queued into one currentEntitlements
            // read. A trigger received during the await gets the next read.
            let request = requested
            let succeeded = publish(await read())
            completed = request

            let completedWaiters = waiters.filter { $0.request <= completed }
            waiters.removeAll { $0.request <= completed }
            for waiter in completedWaiters {
                waiter.continuation.resume(returning: succeeded)
            }
        }
        isRunning = false
    }
}

private struct StoreState {
    var environmentStarted = false
    var storeStarted = false
    // 0 pending, 1 Xcode, 2 sandbox, 3 production, 4 unavailable, 5 unknown
    var environmentState: Int32 = 0
    var products: [Product] = []
    var productsJSON = "[]"
    var productsState: Int32 = 0          // 0 loading, 1 ready, 2 failed
    var purchaseState: Int32 = 0          // 0 idle,1 buying,2 ok,3 fail,4 cancel,5 pending
    var purchaseProduct = ""
    var restoreState: Int32 = 0           // 0 idle, 1 restoring, 2 success, 3 failed
    var entitlementSnapshot = EntitlementSnapshotState()
    var entitlementsJSON = #"{"state":"checking","product_ids":[]}"#
}

final class StoreBridge: @unchecked Sendable {
    static let shared = StoreBridge()

    private let state = OSAllocatedUnfairLock(initialState: StoreState())
    private let entitlementReconciler = EntitlementReconciler(
        read: { await StoreBridge.readEntitlements() },
        publish: { StoreBridge.shared.publishEntitlements($0) }
    )
    private var updatesTask: Task<Void, Never>?
#if canImport(UIKit)
    @MainActor private var foregroundObserver: NSObjectProtocol?
#endif

    // MARK: Commands

    func startEnvironmentResolution() {
        let shouldStart = state.withLock { state -> Bool in
            guard !state.environmentStarted else { return false }
            state.environmentStarted = true
            return true
        }
        guard shouldStart else { return }
        Task { await self.resolveEnvironment() }
    }

    func start(ids: [String]) {
        let shouldStart = state.withLock { state -> Bool in
            guard !state.storeStarted else { return false }
            state.storeStarted = true
            state.productsState = 0
            return true
        }
        guard shouldStart else { return }

        Task { await self.loadProducts(ids) }
        Task { _ = await self.entitlementReconciler.reconcile() }

        // Catch purchases on other devices, Ask-to-Buy approvals, renewals,
        // revocations, and other transactions delivered while the app runs.
        updatesTask = Task.detached { [weak self] in
            for await update in Transaction.updates {
                await self?.handle(update)
            }
        }
        installForegroundRefresh()
    }

    func purchase(_ id: String) {
        let product = state.withLock { state -> Product? in
            state.purchaseProduct = id
            state.purchaseState = 1
            return state.products.first { $0.id == id }
        }
        guard let product else {
            setPurchaseState(3)
            return
        }

        Task {
            do {
                switch try await product.purchase() {
                case .success(let verification):
                    guard case .verified(let transaction) = verification else {
                        self.setPurchaseState(3)
                        return
                    }
                    guard await self.entitlementReconciler.reconcile() else {
                        self.setPurchaseState(3)
                        return
                    }
                    guard transaction.productType == .consumable
                        || self.owns(transaction.productID) else {
                        NSLog("[store] verified purchase missing from reconciled entitlements")
                        self.setPurchaseState(3)
                        return
                    }
                    await transaction.finish()
                    self.setPurchaseState(2)
                case .userCancelled:
                    self.setPurchaseState(4)
                case .pending:
                    self.setPurchaseState(5)
                @unknown default:
                    self.setPurchaseState(3)
                }
            } catch {
                NSLog("[store] purchase failed: %@", String(describing: error))
                self.setPurchaseState(3)
            }
        }
    }

    func clearPurchase() {
        state.withLock {
            $0.purchaseState = 0
            $0.purchaseProduct = ""
        }
    }

    func restore() {
        let shouldStart = state.withLock { state -> Bool in
            guard state.restoreState == 0 else { return false }
            state.restoreState = 1
            return true
        }
        guard shouldStart else { return }

        Task {
            do {
                try await AppStore.sync()
                let reconciled = await self.entitlementReconciler.reconcile()
                self.setRestoreState(reconciled ? 2 : 3)
            } catch {
                NSLog("[store] restore failed: %@", String(describing: error))
                self.setRestoreState(3)
            }
        }
    }

    func clearRestore() {
        state.withLock { $0.restoreState = 0 }
    }

    // MARK: Async work

    private func resolveEnvironment() async {
        do {
            let result = try await AppTransaction.shared
            switch result {
            case .verified(let transaction):
                let environment = transaction.environment
                if environment == .xcode {
                    publishEnvironment(1, logValue: "xcode")
                } else if environment == .sandbox {
                    publishEnvironment(2, logValue: "sandbox")
                } else if environment == .production {
                    publishEnvironment(3, logValue: "production")
                } else {
                    publishEnvironment(5, logValue: "unknown (\(environment.rawValue))")
                }
            case .unverified(_, let error):
                publishEnvironment(
                    4,
                    logValue: "unavailable (verification failed: \(error))"
                )
            }
        } catch {
            publishEnvironment(4, logValue: "unavailable (\(error))")
        }
    }

    private func publishEnvironment(_ value: Int32, logValue: String) {
        state.withLock { $0.environmentState = value }
        NSLog("[store] App Store environment: %@", logValue)
    }

    private func loadProducts(_ ids: [String]) async {
        do {
            let fetched = try await Product.products(for: ids)
            let json = try Self.productsJSON(fetched)
            state.withLock { state in
                state.products = fetched
                state.productsJSON = json
                state.productsState = 1
            }
        } catch {
            NSLog("[store] product load failed: %@", String(describing: error))
            state.withLock { $0.productsState = 2 }
        }
    }

    private func handle(_ result: VerificationResult<Transaction>) async {
        switch result {
        case .verified(let transaction):
            guard await entitlementReconciler.reconcile() else {
                NSLog("[store] leaving transaction unfinished after entitlement reconciliation failed")
                return
            }
            await transaction.finish()
        case .unverified(_, let error):
            NSLog("[store] ignored unverified transaction update: %@", String(describing: error))
        }
    }

    private static func readEntitlements() async -> EntitlementReadResult {
        var owned = Set<String>()
        for await result in Transaction.currentEntitlements {
            switch result {
            case .verified(let transaction) where transaction.revocationDate == nil:
                owned.insert(transaction.productID)
            case .verified:
                break
            case .unverified(_, let error):
                return .failed("verification failed: \(error)")
            }
        }
        return .ready(owned)
    }

    private func publishEntitlements(_ result: EntitlementReadResult) -> Bool {
        switch result {
        case .ready(let owned):
            do {
                let payload = try Self.entitlementJSON(status: .ready, ids: owned)
                state.withLock { state in
                    _ = state.entitlementSnapshot.publishReady(owned)
                    state.entitlementsJSON = payload
                }
                return true
            } catch {
                NSLog("[store] entitlement encoding failed: %@", String(describing: error))
                publishEntitlementFailure()
                return false
            }
        case .failed(let reason):
            NSLog("[store] entitlement reconciliation failed: %@", reason)
            publishEntitlementFailure()
            return false
        }
    }

    private func publishEntitlementFailure() {
        let lastVerified = state.withLock { $0.entitlementSnapshot.owned }
        do {
            let payload = try Self.entitlementJSON(status: .failed, ids: lastVerified)
            state.withLock { state in
                _ = state.entitlementSnapshot.publishFailed()
                state.entitlementsJSON = payload
            }
        } catch {
            // Verified product identifiers are valid Swift strings, so this
            // should be unreachable. Keep the state terminal if Foundation
            // still fails underneath us.
            state.withLock {
                _ = $0.entitlementSnapshot.publishFailed()
                $0.entitlementsJSON = #"{"state":"failed","product_ids":[]}"#
            }
        }
    }

    private func installForegroundRefresh() {
#if canImport(UIKit)
        Task { @MainActor [weak self] in
            guard let self, foregroundObserver == nil else { return }
            foregroundObserver = NotificationCenter.default.addObserver(
                forName: UIApplication.didBecomeActiveNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                Task { _ = await self?.entitlementReconciler.reconcile() }
            }
        }
#endif
    }

    private func setPurchaseState(_ value: Int32) {
        state.withLock { $0.purchaseState = value }
    }

    private func setRestoreState(_ value: Int32) {
        state.withLock { $0.restoreState = value }
    }

    private func owns(_ productID: String) -> Bool {
        state.withLock {
            $0.entitlementSnapshot.status == .ready
                && $0.entitlementSnapshot.owned.contains(productID)
        }
    }

    // MARK: Getters (called from C)

    func environmentStateValue() -> Int32 { state.withLock { $0.environmentState } }
    func productsStateValue() -> Int32 { state.withLock { $0.productsState } }
    func purchaseStateValue() -> Int32 { state.withLock { $0.purchaseState } }
    func restoreStateValue() -> Int32 { state.withLock { $0.restoreState } }
    func entitlementRevisionValue() -> UInt64 {
        state.withLock { $0.entitlementSnapshot.revision }
    }

    // The caller owns every returned allocation and frees it through
    // `store_string_free` after copying.
    func productsJSONValue() -> UnsafeMutablePointer<CChar>? {
        let value = state.withLock { $0.productsJSON }
        return strdup(value)
    }

    func entitlementsJSONValue() -> UnsafeMutablePointer<CChar>? {
        let value = state.withLock { $0.entitlementsJSON }
        return strdup(value)
    }

    func purchaseProductValue() -> UnsafeMutablePointer<CChar>? {
        let value = state.withLock { $0.purchaseProduct }
        return strdup(value)
    }

    // MARK: JSON helpers

    private static func productsJSON(_ products: [Product]) throws -> String {
        let items: [[String: String]] = products.sorted { $0.id < $1.id }.map {
            [
                "id": $0.id,
                "display_name": $0.displayName,
                "display_price": $0.displayPrice,
                "description": $0.description,
            ]
        }
        let data = try JSONSerialization.data(withJSONObject: items, options: [.sortedKeys])
        guard let json = String(data: data, encoding: .utf8) else {
            throw CocoaError(.fileReadInapplicableStringEncoding)
        }
        return json
    }

    private static func entitlementJSON(
        status: EntitlementStatus,
        ids: Set<String>
    ) throws -> String {
        let value: [String: Any] = ["state": status.rawValue, "product_ids": ids.sorted()]
        let data = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
        guard let json = String(data: data, encoding: .utf8) else {
            throw CocoaError(.fileReadInapplicableStringEncoding)
        }
        return json
    }
}

@_cdecl("store_environment_init")
public func store_environment_init() { StoreBridge.shared.startEnvironmentResolution() }

@_cdecl("store_environment_state")
public func store_environment_state() -> Int32 { StoreBridge.shared.environmentStateValue() }

@_cdecl("store_init")
public func store_init(_ ids: UnsafePointer<CChar>) {
    let list = String(cString: ids)
        .split(separator: ",")
        .map { $0.trimmingCharacters(in: .whitespaces) }
        .filter { !$0.isEmpty }
    StoreBridge.shared.start(ids: list)
}

@_cdecl("store_products_state")
public func store_products_state() -> Int32 { StoreBridge.shared.productsStateValue() }

@_cdecl("store_products_json")
public func store_products_json() -> UnsafeMutablePointer<CChar>? {
    StoreBridge.shared.productsJSONValue()
}

@_cdecl("store_purchase")
public func store_purchase(_ id: UnsafePointer<CChar>) {
    StoreBridge.shared.purchase(String(cString: id))
}

@_cdecl("store_purchase_state")
public func store_purchase_state() -> Int32 { StoreBridge.shared.purchaseStateValue() }

@_cdecl("store_purchase_product")
public func store_purchase_product() -> UnsafeMutablePointer<CChar>? {
    StoreBridge.shared.purchaseProductValue()
}

@_cdecl("store_purchase_clear")
public func store_purchase_clear() { StoreBridge.shared.clearPurchase() }

@_cdecl("store_restore")
public func store_restore() { StoreBridge.shared.restore() }

@_cdecl("store_restore_state")
public func store_restore_state() -> Int32 { StoreBridge.shared.restoreStateValue() }

@_cdecl("store_restore_clear")
public func store_restore_clear() { StoreBridge.shared.clearRestore() }

@_cdecl("store_entitlements_rev")
public func store_entitlements_rev() -> UInt64 {
    StoreBridge.shared.entitlementRevisionValue()
}

@_cdecl("store_entitlements_json")
public func store_entitlements_json() -> UnsafeMutablePointer<CChar>? {
    StoreBridge.shared.entitlementsJSONValue()
}

@_cdecl("store_string_free")
public func store_string_free(_ value: UnsafeMutablePointer<CChar>?) { free(value) }

#else
// StoreKit unavailable: linking stubs. Products and entitlements fail closed.

private final class UnavailableStoreBridge: @unchecked Sendable {
    static let shared = UnavailableStoreBridge()
    private let restoreState = OSAllocatedUnfairLock(initialState: Int32(0))

    func restore() { restoreState.withLock { $0 = 3 } }
    func clearRestore() { restoreState.withLock { $0 = 0 } }
    func restoreStateValue() -> Int32 { restoreState.withLock { $0 } }
}

@_cdecl("store_environment_init")
public func store_environment_init() {
    NSLog("[store] App Store environment: unavailable (StoreKit unavailable)")
}
@_cdecl("store_environment_state") public func store_environment_state() -> Int32 { 4 }
@_cdecl("store_init") public func store_init(_ ids: UnsafePointer<CChar>) {}
@_cdecl("store_products_state") public func store_products_state() -> Int32 { 2 }
@_cdecl("store_products_json")
public func store_products_json() -> UnsafeMutablePointer<CChar>? { nil }
@_cdecl("store_purchase") public func store_purchase(_ id: UnsafePointer<CChar>) {}
@_cdecl("store_purchase_state") public func store_purchase_state() -> Int32 { 0 }
@_cdecl("store_purchase_product")
public func store_purchase_product() -> UnsafeMutablePointer<CChar>? { nil }
@_cdecl("store_purchase_clear") public func store_purchase_clear() {}
@_cdecl("store_restore")
public func store_restore() { UnavailableStoreBridge.shared.restore() }
@_cdecl("store_restore_state")
public func store_restore_state() -> Int32 {
    UnavailableStoreBridge.shared.restoreStateValue()
}
@_cdecl("store_restore_clear")
public func store_restore_clear() { UnavailableStoreBridge.shared.clearRestore() }
@_cdecl("store_entitlements_rev") public func store_entitlements_rev() -> UInt64 { 1 }
@_cdecl("store_entitlements_json")
public func store_entitlements_json() -> UnsafeMutablePointer<CChar>? {
    strdup(#"{"state":"failed","product_ids":[]}"#)
}
@_cdecl("store_string_free")
public func store_string_free(_ value: UnsafeMutablePointer<CChar>?) { free(value) }

#endif
