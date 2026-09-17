import Foundation

/// One pending operation owns foreground waits, the system sheet and retries.
/// Injecting platform operations keeps interruption/concurrency tests deterministic.
@MainActor
final class TrackingRequestCoordinator {
    private let isDetermined: () -> Bool
    private let canPresent: () -> Bool
    private let prompt: () async -> Void
    private let pause: () async -> Void
    private var pending: Task<Void, Never>?

    init(
        isDetermined: @escaping () -> Bool,
        canPresent: @escaping () -> Bool,
        prompt: @escaping () async -> Void,
        pause: @escaping () async -> Void = { try? await Task.sleep(for: .seconds(1)) }
    ) {
        self.isDetermined = isDetermined
        self.canPresent = canPresent
        self.prompt = prompt
        self.pause = pause
    }

    func resolve() async {
        guard !isDetermined() else { return }
        if let pending {
            await pending.value
            return
        }
        let task = Task { @MainActor in
            while !isDetermined() {
                if canPresent() {
                    await prompt()
                }
                if !isDetermined() {
                    await pause()
                }
            }
        }
        pending = task
        await task.value
        pending = nil
    }
}
