import XCTest
import os
@testable import Store

private actor ControlledEntitlementReader {
    private var calls = 0
    private var continuations: [CheckedContinuation<EntitlementReadResult, Never>] = []

    func read() async -> EntitlementReadResult {
        calls += 1
        return await withCheckedContinuation { continuation in
            continuations.append(continuation)
        }
    }

    func callCount() -> Int { calls }

    func resolveNext(_ result: EntitlementReadResult) {
        continuations.removeFirst().resume(returning: result)
    }
}

private final class EntitlementPublicationRecorder: @unchecked Sendable {
    private let values = OSAllocatedUnfairLock(initialState: [Set<String>]())

    func publish(_ result: EntitlementReadResult) -> Bool {
        guard case .ready(let owned) = result else { return false }
        values.withLock { $0.append(owned) }
        return true
    }

    func snapshots() -> [Set<String>] { values.withLock { $0 } }
}

final class EntitlementSnapshotTests: XCTestCase {
    func testFirstEmptySnapshotResolves() throws {
        var snapshot = EntitlementSnapshotState()

        XCTAssertTrue(snapshot.publishReady([]))
        XCTAssertEqual(snapshot.status, .ready)
        XCTAssertTrue(snapshot.owned.isEmpty)
        XCTAssertEqual(snapshot.revision, 1)
    }

    func testRevocationPublishesAuthoritativeEmptySnapshot() throws {
        var snapshot = EntitlementSnapshotState()
        _ = snapshot.publishReady(["com.example.supporter"])

        XCTAssertTrue(snapshot.publishReady([]))
        XCTAssertEqual(snapshot.status, .ready)
        XCTAssertTrue(snapshot.owned.isEmpty)
        XCTAssertEqual(snapshot.revision, 2)
    }

    func testFailureRetainsLastVerifiedOwnership() throws {
        var snapshot = EntitlementSnapshotState()
        _ = snapshot.publishReady(["com.example.supporter"])

        XCTAssertTrue(snapshot.publishFailed())
        XCTAssertEqual(snapshot.status, .failed)
        XCTAssertEqual(snapshot.owned, ["com.example.supporter"])
        XCTAssertEqual(snapshot.revision, 2)
    }

    func testUnchangedReadySnapshotDoesNotInventAnotherEvent() {
        var snapshot = EntitlementSnapshotState()
        _ = snapshot.publishReady([])

        XCTAssertFalse(snapshot.publishReady([]))
        XCTAssertEqual(snapshot.revision, 1)
    }

    func testTriggerDuringReadIsSerializedBehindCurrentSnapshot() async {
        let reader = ControlledEntitlementReader()
        let recorder = EntitlementPublicationRecorder()
        let reconciler = EntitlementReconciler(
            read: { await reader.read() },
            publish: { recorder.publish($0) }
        )

        let first = Task { await reconciler.reconcile() }
        await waitForCallCount(1, reader: reader)

        let second = Task { await reconciler.reconcile() }
        await Task.yield()
        let callsDuringFirstRead = await reader.callCount()
        XCTAssertEqual(callsDuringFirstRead, 1)

        await reader.resolveNext(.ready(["com.example.previous"]))
        await waitForCallCount(2, reader: reader)
        await reader.resolveNext(.ready(["com.example.current"]))

        let firstSucceeded = await first.value
        let secondSucceeded = await second.value
        XCTAssertTrue(firstSucceeded)
        XCTAssertTrue(secondSucceeded)
        XCTAssertEqual(
            recorder.snapshots(),
            [Set(["com.example.previous"]), Set(["com.example.current"])]
        )
    }

    private func waitForCallCount(
        _ expected: Int,
        reader: ControlledEntitlementReader
    ) async {
        for _ in 0..<1_000 {
            if await reader.callCount() == expected { return }
            await Task.yield()
        }
        XCTFail("timed out waiting for entitlement read \(expected)")
    }
}
