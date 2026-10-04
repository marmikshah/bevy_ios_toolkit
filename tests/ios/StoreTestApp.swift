import Foundation
import Store
import SwiftUI

/// A native host for the production Store bridge. No game renderer is needed
/// to exercise StoreKit transactions and the C ABI's polled results.
@main
struct StoreTestApp: App {
    private static let productID = "iap.playground.removeads"

    private struct ProductSnapshot: Decodable {
        let id: String
        let display_price: String
    }

    private struct EntitlementSnapshot: Decodable {
        let state: String
        let product_ids: [String]
    }

    @State private var started = false
    @State private var catalogue: Int32 = 0
    @State private var products: [ProductSnapshot] = []
    @State private var entitlements = EntitlementSnapshot(state: "checking", product_ids: [])
    @State private var purchase: Int32 = 0
    @State private var restore: Int32 = 0

    private var owned: Bool {
        entitlements.state == "ready" && entitlements.product_ids.contains(Self.productID)
    }

    private var price: String {
        if catalogue == 2 { return "unavailable" }
        return products.first { $0.id == Self.productID }?.display_price ?? "loading"
    }

    var body: some Scene {
        WindowGroup {
            VStack(spacing: 16) {
                Text("Toolkit StoreKit")
                Text("store: \(store_environment_state() == 0 ? "pending" : "resolved")")
                Text("entitlements: \(entitlements.state.capitalized)")
                Text("owned: \(owned ? "true" : "false")")
                Text("price: \(price)").accessibilityIdentifier("store.price")
                Text("purchase: \([0: "Idle", 1: "Buying", 2: "Success", 3: "Failed", 4: "Cancelled", 5: "Pending"][purchase] ?? "Invalid")")
                Text("restore: \([0: "Idle", 1: "Restoring", 2: "Success", 3: "Failed"][restore] ?? "Invalid")")
                Button("Buy: \(price)") { Self.productID.withCString(store_purchase) }
                    .accessibilityIdentifier("store.buy")
                    .disabled(catalogue != 1 || price == "loading" || entitlements.state != "ready" || owned)
                Button("Restore Purchases") { store_restore() }
                    .accessibilityIdentifier("store.restore")
                Button("Retry Store") { store_reload() }
                    .accessibilityIdentifier("store.retry")
            }
            .padding(24)
            .onAppear {
                guard !started else { return }
                started = true
                Self.productID.withCString(store_init)
            }
            .onReceive(Timer.publish(every: 0.1, on: .main, in: .common).autoconnect()) { _ in
                catalogue = store_products_state()
                products = read(store_products_json(), as: [ProductSnapshot].self) ?? []
                entitlements = read(store_entitlements_json(), as: EntitlementSnapshot.self)
                    ?? EntitlementSnapshot(state: "invalid", product_ids: [])
                // Match the C ABI consumer: retain terminal outcomes for the
                // UI and clear the native slot so the next request can run.
                let purchaseState = store_purchase_state()
                if purchaseState != 0 {
                    purchase = purchaseState
                    if purchaseState >= 2 { store_purchase_clear() }
                }
                let restoreState = store_restore_state()
                if restoreState != 0 {
                    restore = restoreState
                    if restoreState >= 2 { store_restore_clear() }
                }
            }
        }
    }

    private func read<Value: Decodable>(_ pointer: UnsafeMutablePointer<CChar>?, as type: Value.Type) -> Value? {
        guard let pointer else { return nil }
        defer { store_string_free(pointer) }
        return try? JSONDecoder().decode(type, from: Data(String(cString: pointer).utf8))
    }
}
