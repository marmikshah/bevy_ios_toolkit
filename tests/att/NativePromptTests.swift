import Foundation

@MainActor
enum NativePromptTests {
    static func run() async {
        let coordinator = NativePromptCoordinator()
        let first = coordinator.tryBegin(.consent)!
        precondition(coordinator.tryBegin(.purchase) == nil)
        var waiting = false
        let task = Task { @MainActor in
            await coordinator.begin(.notifications, canPresent: { true }, pause: {
                waiting = true
                await Task.yield()
            })
        }
        while !waiting { await Task.yield() }
        precondition(coordinator.kind == .consent)
        coordinator.end(first)
        let second = await task.value!
        precondition(coordinator.kind == .notifications)
        // A late callback from the previous SDK cannot release the next sheet.
        coordinator.end(first)
        precondition(coordinator.kind == .notifications)
        coordinator.end(second)
        precondition(coordinator.isIdle)

        var active = false
        let third = await coordinator.begin(.tracking, canPresent: { active }, pause: {
            precondition(coordinator.isIdle)
            active = true
            await Task.yield()
        })!
        precondition(coordinator.kind == .tracking)
        let cancelled = Task { @MainActor in
            await coordinator.begin(.restore, canPresent: { true })
        }
        cancelled.cancel()
        let cancellation = await cancelled.value
        precondition(cancellation == nil)
        precondition(coordinator.kind == .tracking)
        coordinator.end(third)
        print("NATIVE_PROMPT_TESTS_PASSED: serialization, active presenter, cancellation, stale completion")
    }
}
