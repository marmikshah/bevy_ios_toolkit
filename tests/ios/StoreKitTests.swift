import StoreKitTest
import XCTest

/// Real StoreKit transactions and production bridge results through native UI.
/// Rust tests separately exercise the consumer's projection and messages.
@MainActor
final class StoreKitTests: XCTestCase {
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
        let app = XCUIApplication(bundleIdentifier: "com.marmikshah.toolkit.store-tests")
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 30))
        return app
    }

    private func waitForStatus(_ text: String, in app: XCUIApplication) throws {
        let status = app.staticTexts.matching(NSPredicate(format: "label == %@", text)).firstMatch
        guard status.waitForExistence(timeout: 30) else {
            XCTFail("Missing native bridge status: \(text)")
            throw NSError(domain: "StoreKitTests", code: 1)
        }
    }

    private func waitForButton(_ identifier: String, in app: XCUIApplication) throws -> XCUIElement {
        let button = app.buttons[identifier]
        let ready = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "exists == true AND enabled == true AND hittable == true"), object: button
        )
        guard XCTWaiter.wait(for: [ready], timeout: 30) == .completed else {
            XCTFail("Missing or disabled native control: \(identifier)")
            throw NSError(domain: "StoreKitTests", code: 2)
        }
        return button
    }

    private func tap(_ identifier: String, in app: XCUIApplication) throws {
        try waitForButton(identifier, in: app).tap()
    }

    func testQuietLaunchPurchaseRestoreAndRelaunch() throws {
        let store = try session()
        defer { store.resetToDefaultState() }
        let app = launch()
        try waitForStatus("entitlements: Ready", in: app)
        try waitForStatus("store: pending", in: app)
        _ = try waitForButton("store.buy", in: app)
        XCTAssertTrue(app.staticTexts["store.price"].label.contains("0.99"),
                      "The bridge did not publish the configured localized price")
        XCTAssertFalse(app.alerts.firstMatch.exists, "Quiet initialization must not request an account sync")
        try tap("store.restore", in: app)
        try waitForStatus("restore: Success", in: app)
        try waitForStatus("owned: false", in: app)
        try tap("store.buy", in: app)
        try waitForStatus("owned: true", in: app)
        XCTAssertEqual(store.allTransactions().count, 1)
        XCTAssertEqual(store.allTransactions().first?.productIdentifier, product)
        app.terminate()
        app.launch()
        try waitForStatus("owned: true", in: app)
        try tap("store.restore", in: app)
        try waitForStatus("restore: Success", in: app)
        try waitForStatus("owned: true", in: app)
    }

    func testRetryFailureCancellationAndPendingApproval() async throws {
        let store = try session()
        defer { store.resetToDefaultState() }
        try await store.setSimulatedError(.generic(.unknown), forAPI: .loadProducts)
        let app = launch()
        try waitForStatus("price: unavailable", in: app)
        try waitForStatus("entitlements: Ready", in: app)
        try await store.setSimulatedError(nil, forAPI: .loadProducts)
        try tap("store.retry", in: app)
        _ = try waitForButton("store.buy", in: app)
        try await store.setSimulatedError(.generic(.unknown), forAPI: .purchase)
        try tap("store.buy", in: app)
        try waitForStatus("purchase: Failed", in: app)
        try waitForStatus("owned: false", in: app)
        try await store.setSimulatedError(.generic(.userCancelled), forAPI: .purchase)
        try tap("store.buy", in: app)
        try waitForStatus("purchase: Cancelled", in: app)
        try await store.setSimulatedError(nil, forAPI: .purchase)
        store.askToBuyEnabled = true
        try tap("store.buy", in: app)
        try waitForStatus("purchase: Pending", in: app)
        try waitForStatus("owned: false", in: app)
        let pending = try XCTUnwrap(store.allTransactions().last)
        XCTAssertEqual(pending.productIdentifier, product)
        try store.approveAskToBuyTransaction(identifier: pending.identifier)
        try waitForStatus("owned: true", in: app)
        try waitForStatus("entitlements: Ready", in: app)
    }

    func testExistingPurchaseAndRefundAreAuthoritative() throws {
        let store = try session()
        defer { store.resetToDefaultState() }
        // Establish the real purchase through the app. SKTestSession's
        // off-device buy can create a transaction without returning its JWS.
        let app = launch()
        try waitForStatus("owned: false", in: app)
        try tap("store.buy", in: app)
        try waitForStatus("owned: true", in: app)
        app.terminate()
        app.launch()
        try waitForStatus("entitlements: Ready", in: app)
        try waitForStatus("owned: true", in: app)
        let transaction = try XCTUnwrap(store.allTransactions().first { $0.productIdentifier == product })
        try store.refundTransaction(identifier: transaction.identifier)
        try waitForStatus("owned: false", in: app)
        try waitForStatus("entitlements: Ready", in: app)
        try tap("store.restore", in: app)
        try waitForStatus("restore: Success", in: app)
        try waitForStatus("owned: false", in: app)
        app.terminate()
        app.launch()
        try waitForStatus("entitlements: Ready", in: app)
        try waitForStatus("owned: false", in: app)
        _ = try waitForButton("store.buy", in: app)
    }
}
