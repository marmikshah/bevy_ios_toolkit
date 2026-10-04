import StoreKitTest
import XCTest

/// Real StoreKit transactions and native bridge results, through the Bevy demo.
/// Games keep only product benefits and UI integration checks.
@MainActor
final class StoreKitTests: RenderedDemoTestCase {
    private let product = "iap.playground.removeads"

    private func session() throws -> SKTestSession {
        continueAfterFailure = false
        let store = try SKTestSession(configurationFileNamed: "Toolkit")
        store.resetToDefaultState()
        store.clearTransactions()
        store.disableDialogs = true
        XCTAssertTrue(store.disableDialogs)
        return store
    }

    private func launch() -> XCUIApplication {
        let app = XCUIApplication(bundleIdentifier: "com.marmikshah.playground")
        app.launchEnvironment["BEVY_DEMO_STORE_TEST"] = "1"
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 30))
        return app
    }

    func testQuietLaunchPurchaseRestoreAndRelaunch() throws {
        let store = try session()
        defer { store.resetToDefaultState() }
        let app = launch()
        _ = try waitForStatus("entitlements: Ready", in: app)
        _ = try waitForStatus("store: pending", in: app)
        XCTAssertFalse(app.alerts.firstMatch.exists, "Quiet initialization must not request an account sync")
        try tap("Restore Purchases", in: app)
        _ = try waitForStatus("restore: Success", in: app)
        _ = try waitForStatus("owned: false", in: app)
        try tap("Buy:", in: app)
        _ = try waitForStatus("owned: true", in: app)
        XCTAssertEqual(store.allTransactions().count, 1)
        XCTAssertEqual(store.allTransactions().first?.productIdentifier, product)
        app.terminate()
        app.launch()
        _ = try waitForStatus("owned: true", in: app)
        try tap("Restore Purchases", in: app)
        _ = try waitForStatus("restore: Success", in: app)
        _ = try waitForStatus("owned: true", in: app)
    }

    func testRetryFailureCancellationAndPendingApproval() async throws {
        let store = try session()
        defer { store.resetToDefaultState() }
        try await store.setSimulatedError(.generic(.unknown), forAPI: .loadProducts)
        let app = launch()
        _ = try waitForStatus("price: unavailable", in: app)
        _ = try waitForStatus("entitlements: Ready", in: app)
        try await store.setSimulatedError(nil, forAPI: .loadProducts)
        try tap("Retry Store", in: app)
        _ = try waitFor("Buy:", in: app)
        try await store.setSimulatedError(.generic(.unknown), forAPI: .purchase)
        try tap("Buy:", in: app)
        _ = try waitForStatus("purchase: Failed", in: app)
        _ = try waitForStatus("owned: false", in: app)
        try await store.setSimulatedError(.generic(.userCancelled), forAPI: .purchase)
        try tap("Buy:", in: app)
        _ = try waitForStatus("purchase: Cancelled", in: app)
        try await store.setSimulatedError(nil, forAPI: .purchase)
        store.askToBuyEnabled = true
        try tap("Buy:", in: app)
        _ = try waitForStatus("purchase: Pending", in: app)
        _ = try waitForStatus("owned: false", in: app)
        let pending = try XCTUnwrap(store.allTransactions().last)
        XCTAssertEqual(pending.productIdentifier, product)
        try store.approveAskToBuyTransaction(identifier: pending.identifier)
        _ = try waitForStatus("owned: true", in: app)
    }

    func testExistingPurchaseAndRefundAreAuthoritative() throws {
        let store = try session()
        defer { store.resetToDefaultState() }
        // Establish the real purchase through the app. SKTestSession's
        // off-device buy can create a transaction without returning its JWS.
        let app = launch()
        _ = try waitForStatus("owned: false", in: app)
        try tap("Buy:", in: app)
        _ = try waitForStatus("owned: true", in: app)
        app.terminate()
        app.launch()
        _ = try waitForStatus("entitlements: Ready", in: app)
        _ = try waitForStatus("owned: true", in: app)
        let transaction = try XCTUnwrap(store.allTransactions().first { $0.productIdentifier == product })
        try store.refundTransaction(identifier: transaction.identifier)
        _ = try waitForStatus("owned: false", in: app)
        try tap("Restore Purchases", in: app)
        _ = try waitForStatus("restore: Success", in: app)
        _ = try waitForStatus("owned: false", in: app)
        app.terminate()
        app.launch()
        _ = try waitForStatus("entitlements: Ready", in: app)
        _ = try waitForStatus("owned: false", in: app)
        _ = try waitFor("Buy:", in: app)
    }
}
