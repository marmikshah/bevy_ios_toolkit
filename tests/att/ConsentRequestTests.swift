import Foundation

@MainActor
enum ConsentRequestTests {
    enum Failure: Error { case offline, presentation }

    static func run() async {
        await savedDecisionsAreVerifiedWithoutPrompting()
        await expiredDecisionIsAskedAfterVerification()
        await waitsForForegroundAndPresentationHost()
        await coalescesConcurrentVerification()
        await cachedPermissionSurvivesRefreshFailure()
        await retriesTransientRefreshFailure()
        await persistentFailureHasBoundedRetries()
        await retriesFailedForm()
        await answeredFormFailureDoesNotAskAgain()
        await gameProgressContinuesWhileFormIsPending()
        print("UMP_TESTS_PASSED: launch verification, saved decisions, foreground waits, coalescing, bounded retries, game progress")
    }

    static func savedDecisionsAreVerifiedWithoutPrompting() async {
        for choice in ["approved", "rejected", "not-required"] {
            var refreshes = 0
            let request = ConsentRequestCoordinator(
                refresh: { refreshes += 1 },
                requiresConsent: { false },
                canRequestAds: { choice != "rejected" },
                canPresent: { preconditionFailure("saved choice must not prompt") },
                prompt: { preconditionFailure("saved choice must not prompt") }
            )
            await request.resolve()
            await request.resolve()
            precondition(refreshes == 1, "verify \(choice) once on this launch")
        }
    }

    static func expiredDecisionIsAskedAfterVerification() async {
        var required = false
        var refreshes = 0
        var prompts = 0
        let request = ConsentRequestCoordinator(
            refresh: {
                refreshes += 1
                required = true // UMP says the previous decision expired.
            },
            requiresConsent: { required },
            canRequestAds: { false },
            canPresent: { true },
            prompt: {
                precondition(refreshes == 1, "verify before collecting a decision")
                prompts += 1
                required = false
            }
        )
        await request.resolve()
        await request.resolve()
        precondition(refreshes == 1 && prompts == 1)
    }

    static func waitsForForegroundAndPresentationHost() async {
        var active = false
        var rendered = false
        var modal = true
        var required = true
        var pauses = 0
        var prompts = 0
        let request = ConsentRequestCoordinator(
            refresh: {},
            requiresConsent: { required },
            canRequestAds: { false },
            canPresent: { active && rendered && !modal },
            prompt: {
                precondition(active && rendered && !modal)
                prompts += 1
                required = false
            },
            pause: {
                pauses += 1
                precondition(prompts == 0)
                switch pauses {
                case 1: active = true
                case 2: rendered = true
                case 3: modal = false
                default: preconditionFailure("presentation did not become ready")
                }
                await Task.yield()
            }
        )
        await request.resolve()
        precondition(pauses == 3 && prompts == 1)
    }

    static func coalescesConcurrentVerification() async {
        var required = true
        var refreshes = 0
        var prompts = 0
        var releaseRefresh: CheckedContinuation<Void, Never>?
        let request = ConsentRequestCoordinator(
            refresh: {
                refreshes += 1
                await withCheckedContinuation { releaseRefresh = $0 }
            },
            requiresConsent: { required },
            canRequestAds: { false },
            canPresent: { true },
            prompt: {
                prompts += 1
                required = false
            }
        )
        let first = Task { await request.resolve() }
        while releaseRefresh == nil { await Task.yield() }
        var joined = 0
        let others = (0..<20).map { _ in
            Task {
                joined += 1
                await request.resolve()
            }
        }
        while joined != others.count { await Task.yield() }
        precondition(refreshes == 1 && prompts == 0)
        releaseRefresh?.resume()
        await first.value
        for other in others { await other.value }
        precondition(refreshes == 1 && prompts == 1)
    }

    static func cachedPermissionSurvivesRefreshFailure() async {
        var refreshes = 0
        let request = ConsentRequestCoordinator(
            refresh: {
                refreshes += 1
                throw Failure.offline
            },
            requiresConsent: { true },
            canRequestAds: { true },
            canPresent: { preconditionFailure("failed refresh must not prompt") },
            prompt: { preconditionFailure("failed refresh must not prompt") },
            retry: { _ in preconditionFailure("valid cached permission needs no retry") }
        )
        await request.resolve()
        precondition(refreshes == 1)
    }

    static func retriesTransientRefreshFailure() async {
        var required = true
        var refreshes = 0
        var prompts = 0
        var retries: [Int] = []
        let request = ConsentRequestCoordinator(
            refresh: {
                refreshes += 1
                if refreshes == 1 { throw Failure.offline }
            },
            requiresConsent: { required },
            canRequestAds: { false },
            canPresent: { true },
            prompt: {
                precondition(refreshes == 2)
                prompts += 1
                required = false
            },
            retry: { retries.append($0); await Task.yield() }
        )
        await request.resolve()
        precondition(refreshes == 2 && prompts == 1 && retries == [0])
    }

    static func persistentFailureHasBoundedRetries() async {
        var refreshes = 0
        var retries: [Int] = []
        let request = ConsentRequestCoordinator(
            refresh: {
                refreshes += 1
                throw Failure.offline
            },
            requiresConsent: { true },
            canRequestAds: { false },
            canPresent: { preconditionFailure("offline verification must not prompt") },
            prompt: { preconditionFailure("offline verification must not prompt") },
            retry: { retries.append($0); await Task.yield() }
        )
        await request.resolve()
        precondition(refreshes == 4 && retries == [0, 1, 2])
    }

    static func retriesFailedForm() async {
        var required = true
        var prompts = 0
        var retries: [Int] = []
        let request = ConsentRequestCoordinator(
            refresh: {},
            requiresConsent: { required },
            canRequestAds: { false },
            canPresent: { true },
            prompt: {
                prompts += 1
                if prompts == 1 { throw Failure.presentation }
                required = false
            },
            retry: { retries.append($0); await Task.yield() }
        )
        await request.resolve()
        await request.resolve()
        precondition(prompts == 2 && retries == [0])
    }

    static func answeredFormFailureDoesNotAskAgain() async {
        var required = true
        var prompts = 0
        let request = ConsentRequestCoordinator(
            refresh: {},
            requiresConsent: { required },
            canRequestAds: { false },
            canPresent: { true },
            prompt: {
                prompts += 1
                required = false
                throw Failure.presentation
            },
            retry: { _ in preconditionFailure("answered decisions must not prompt again") }
        )
        await request.resolve()
        await request.resolve()
        precondition(prompts == 1)
    }

    static func gameProgressContinuesWhileFormIsPending() async {
        var required = true
        var releaseForm: CheckedContinuation<Void, Never>?
        let request = ConsentRequestCoordinator(
            refresh: {},
            requiresConsent: { required },
            canRequestAds: { false },
            canPresent: { true },
            prompt: {
                await withCheckedContinuation { releaseForm = $0 }
                required = false
            }
        )
        let operation = Task { await request.resolve() }
        while releaseForm == nil { await Task.yield() }
        var frames = 0
        for _ in 0..<120 {
            frames += 1
            await Task.yield()
        }
        precondition(frames == 120 && required)
        releaseForm?.resume()
        await operation.value
        precondition(!required)
    }
}
