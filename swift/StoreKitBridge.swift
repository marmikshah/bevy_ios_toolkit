// StoreKit 2 bridge for bevy_cupertino.
//
// Design contract with the Rust side (bevy_cupertino::store):
//   - Every entry point is @_cdecl C-ABI, called FROM Rust.
//   - StoreKit's async work runs in Tasks; results surface as POLLED state
//     (Int32 / C-string getters), never callbacks into Rust — re-entrancy
//     against winit's event loop is not safe. This mirrors NativeBridge.swift.
//   - *_json getters return pointers to buffers owned here, valid only until
//     the next regenerating call; Rust copies immediately.
//   - This file must COMPILE AND LINK even where StoreKit is unavailable:
//     everything StoreKit-specific sits behind #if canImport(StoreKit), with
//     linking stubs otherwise.
//
// Integration: add this file to the app's Xcode target alongside
// NativeBridge.swift. StoreKit is a system framework (auto-linked, no SPM).
// Symbol prefix `cupertino_` avoids collision with the game's own bridge.
//
// Deployment floor iOS 26 — StoreKit 2 (iOS 15+) needs no availability guards.
// All shared state lives behind an OSAllocatedUnfairLock so the async StoreKit
// tasks and the synchronous C-ABI getters stay race-free and Swift-6 clean.

import Foundation
import os

#if canImport(StoreKit)
import StoreKit

private struct StoreState {
    var products: [Product] = []
    var productsState: Int32 = 0          // 0 loading, 1 ready, 2 failed
    var purchaseState: Int32 = 0          // 0 idle,1 buying,2 ok,3 fail,4 cancel,5 pending
    var purchaseProduct: String = ""
    var entitled: Set<String> = []
    var entRev: UInt64 = 0
    // Owned C-string buffers; freed when regenerated.
    var productsJSONPtr: UnsafeMutablePointer<CChar>?
    var entitlementsJSONPtr: UnsafeMutablePointer<CChar>?
    var purchaseProductPtr: UnsafeMutablePointer<CChar>?
}

final class CupertinoStore: @unchecked Sendable {
    static let shared = CupertinoStore()

    private let state = OSAllocatedUnfairLock(initialState: StoreState())
    private var updatesTask: Task<Void, Never>?

    // MARK: Commands

    func start(ids: [String]) {
        state.withLock { $0.productsState = 0 }
        Task { await self.loadProducts(ids) }
        Task { await self.refreshEntitlements() }
        // Catch purchases on other devices / Ask-to-Buy approvals / renewals.
        updatesTask = Task.detached { [weak self] in
            for await update in Transaction.updates {
                await self?.handle(update)
            }
        }
    }

    func purchase(_ id: String) {
        let product = state.withLock { s -> Product? in
            s.purchaseProduct = id
            s.purchaseState = 1
            return s.products.first { $0.id == id }
        }
        guard let product else {
            setPurchaseState(3)
            return
        }
        Task {
            do {
                switch try await product.purchase() {
                case .success(let verification):
                    if case .verified(let transaction) = verification {
                        await transaction.finish()
                        await self.refreshEntitlements()
                        self.setPurchaseState(2)
                    } else {
                        self.setPurchaseState(3)
                    }
                case .userCancelled:
                    self.setPurchaseState(4)
                case .pending:
                    self.setPurchaseState(5)
                @unknown default:
                    self.setPurchaseState(3)
                }
            } catch {
                NSLog("[cupertino] purchase failed: %@", String(describing: error))
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
        Task {
            try? await AppStore.sync()
            await self.refreshEntitlements()
        }
    }

    // MARK: Async work

    private func loadProducts(_ ids: [String]) async {
        do {
            let fetched = try await Product.products(for: ids)
            let json = Self.productsJSON(fetched)
            state.withLock { s in
                s.products = fetched
                if let old = s.productsJSONPtr { free(old) }
                s.productsJSONPtr = strdup(json)
                s.productsState = 1
            }
        } catch {
            NSLog("[cupertino] product load failed: %@", String(describing: error))
            state.withLock { $0.productsState = 2 }
        }
    }

    private func handle(_ result: VerificationResult<Transaction>) async {
        if case .verified(let transaction) = result {
            await transaction.finish()
            await refreshEntitlements()
        }
    }

    private func refreshEntitlements() async {
        var owned = Set<String>()
        for await result in Transaction.currentEntitlements {
            if case .verified(let transaction) = result, transaction.revocationDate == nil {
                owned.insert(transaction.productID)
            }
        }
        let json = Self.idsJSON(owned)
        let snapshot = owned
        state.withLock { s in
            if snapshot != s.entitled {
                s.entitled = snapshot
                if let old = s.entitlementsJSONPtr { free(old) }
                s.entitlementsJSONPtr = strdup(json)
                s.entRev &+= 1
            }
        }
    }

    private func setPurchaseState(_ value: Int32) {
        state.withLock { $0.purchaseState = value }
    }

    // MARK: Getters (called from C)

    func productsStateValue() -> Int32 { state.withLock { $0.productsState } }
    func purchaseStateValue() -> Int32 { state.withLock { $0.purchaseState } }
    func entRevValue() -> UInt64 { state.withLock { $0.entRev } }

    // withLockUnchecked: the result is a raw pointer (not Sendable), but it's
    // only read synchronously by the C caller, which copies immediately.
    func productsJSONValue() -> UnsafePointer<CChar>? {
        state.withLockUnchecked { s in s.productsJSONPtr.map { UnsafePointer($0) } }
    }
    func entitlementsJSONValue() -> UnsafePointer<CChar>? {
        state.withLockUnchecked { s in s.entitlementsJSONPtr.map { UnsafePointer($0) } }
    }
    func purchaseProductValue() -> UnsafePointer<CChar>? {
        state.withLockUnchecked { s in
            if let old = s.purchaseProductPtr { free(old) }
            s.purchaseProductPtr = strdup(s.purchaseProduct)
            return s.purchaseProductPtr.map { UnsafePointer($0) }
        }
    }

    // MARK: JSON helpers

    private static func productsJSON(_ products: [Product]) -> String {
        let items: [[String: String]] = products.map {
            [
                "id": $0.id,
                "display_name": $0.displayName,
                "display_price": $0.displayPrice,
                "description": $0.description,
            ]
        }
        return (try? JSONSerialization.data(withJSONObject: items))
            .flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
    }
    private static func idsJSON(_ ids: Set<String>) -> String {
        return (try? JSONSerialization.data(withJSONObject: Array(ids)))
            .flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
    }
}

@_cdecl("cupertino_store_init")
public func cupertino_store_init(_ ids: UnsafePointer<CChar>) {
    let list = String(cString: ids)
        .split(separator: ",")
        .map { $0.trimmingCharacters(in: .whitespaces) }
        .filter { !$0.isEmpty }
    CupertinoStore.shared.start(ids: list)
}

@_cdecl("cupertino_store_products_state")
public func cupertino_store_products_state() -> Int32 { CupertinoStore.shared.productsStateValue() }

@_cdecl("cupertino_store_products_json")
public func cupertino_store_products_json() -> UnsafePointer<CChar>? { CupertinoStore.shared.productsJSONValue() }

@_cdecl("cupertino_store_purchase")
public func cupertino_store_purchase(_ id: UnsafePointer<CChar>) { CupertinoStore.shared.purchase(String(cString: id)) }

@_cdecl("cupertino_store_purchase_state")
public func cupertino_store_purchase_state() -> Int32 { CupertinoStore.shared.purchaseStateValue() }

@_cdecl("cupertino_store_purchase_product")
public func cupertino_store_purchase_product() -> UnsafePointer<CChar>? { CupertinoStore.shared.purchaseProductValue() }

@_cdecl("cupertino_store_purchase_clear")
public func cupertino_store_purchase_clear() { CupertinoStore.shared.clearPurchase() }

@_cdecl("cupertino_store_restore")
public func cupertino_store_restore() { CupertinoStore.shared.restore() }

@_cdecl("cupertino_store_entitlements_rev")
public func cupertino_store_entitlements_rev() -> UInt64 { CupertinoStore.shared.entRevValue() }

@_cdecl("cupertino_store_entitlements_json")
public func cupertino_store_entitlements_json() -> UnsafePointer<CChar>? { CupertinoStore.shared.entitlementsJSONValue() }

#else
// StoreKit unavailable: linking stubs. Products report failed, nothing owned.

@_cdecl("cupertino_store_init") public func cupertino_store_init(_ ids: UnsafePointer<CChar>) {}
@_cdecl("cupertino_store_products_state") public func cupertino_store_products_state() -> Int32 { 2 }
@_cdecl("cupertino_store_products_json") public func cupertino_store_products_json() -> UnsafePointer<CChar>? { nil }
@_cdecl("cupertino_store_purchase") public func cupertino_store_purchase(_ id: UnsafePointer<CChar>) {}
@_cdecl("cupertino_store_purchase_state") public func cupertino_store_purchase_state() -> Int32 { 0 }
@_cdecl("cupertino_store_purchase_product") public func cupertino_store_purchase_product() -> UnsafePointer<CChar>? { nil }
@_cdecl("cupertino_store_purchase_clear") public func cupertino_store_purchase_clear() {}
@_cdecl("cupertino_store_restore") public func cupertino_store_restore() {}
@_cdecl("cupertino_store_entitlements_rev") public func cupertino_store_entitlements_rev() -> UInt64 { 0 }
@_cdecl("cupertino_store_entitlements_json") public func cupertino_store_entitlements_json() -> UnsafePointer<CChar>? { nil }

#endif
