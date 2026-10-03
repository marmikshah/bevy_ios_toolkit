import Foundation

/// Verify once per launch, then collect only an outstanding UMP decision.
/// SDK and presentation waits suspend a task; they never wait on the game loop.
@MainActor
final class ConsentRequestCoordinator {
    private let refresh: () async throws -> Void
    private let requiresConsent: () -> Bool
    private let canRequestAds: () -> Bool
    private let canPresent: () -> Bool
    private let prompt: () async throws -> Void
    private let pause: () async -> Void
    private let retry: (Int) async -> Void
    private var verified = false
    private var pending: Task<Void, Never>?

    init(
        refresh: @escaping () async throws -> Void,
        requiresConsent: @escaping () -> Bool,
        canRequestAds: @escaping () -> Bool,
        canPresent: @escaping () -> Bool,
        prompt: @escaping () async throws -> Void,
        pause: @escaping () async -> Void = { try? await Task.sleep(for: .seconds(1)) },
        retry: @escaping (Int) async -> Void = {
            try? await Task.sleep(for: .seconds(15 << $0))
        }
    ) {
        self.refresh = refresh
        self.requiresConsent = requiresConsent
        self.canRequestAds = canRequestAds
        self.canPresent = canPresent
        self.prompt = prompt
        self.pause = pause
        self.retry = retry
    }

    func resolve() async {
        if let pending {
            await pending.value
            return
        }
        guard !verified || requiresConsent() else { return }
        let task = Task { @MainActor in
            // A failed refresh or form gets at most three background retries.
            for attempt in 0...3 {
                do {
                    try await refresh()
                    verified = true
                } catch {
                    // UMP may still permit ads using a valid previous decision.
                    // Never present a form based on an unverified cached status.
                    guard !canRequestAds(), attempt < 3 else { return }
                    await retry(attempt)
                    continue
                }
                guard requiresConsent() else { return }
                while !canPresent() {
                    await pause()
                }
                // A privacy-options interaction may have resolved it meanwhile.
                guard requiresConsent() else { return }
                do {
                    try await prompt()
                    return
                } catch {
                    guard requiresConsent(), attempt < 3 else { return }
                    await retry(attempt)
                }
            }
        }
        pending = task
        await task.value
        pending = nil
    }
}
