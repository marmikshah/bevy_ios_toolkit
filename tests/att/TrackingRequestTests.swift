import Foundation

@main
@MainActor
enum TrackingRequestTests {
    static func main() async {
        await skipsExistingDecision()
        await waitsAndRetriesInterruptedRequest()
        await coalescesConcurrentRequests()
        print("ATT_TESTS_PASSED: existing decisions, foreground/presentation waits, interruption retry, coalescing")
    }

    static func skipsExistingDecision() async {
        let request = TrackingRequestCoordinator(
            isDetermined: { true },
            canPresent: { preconditionFailure("already answered") },
            prompt: { preconditionFailure("already answered") }
        )
        await request.resolve()
    }

    static func waitsAndRetriesInterruptedRequest() async {
        var active = false
        var presenting = true
        var determined = false
        var pauses = 0
        var prompts = 0
        let request = TrackingRequestCoordinator(
            isDetermined: { determined },
            canPresent: { active && !presenting },
            prompt: {
                precondition(active && !presenting)
                prompts += 1
                if prompts == 1 {
                    // iOS dropped the call during a background interruption.
                    active = false
                } else {
                    determined = true
                }
            },
            pause: {
                pauses += 1
                switch pauses {
                case 1:
                    precondition(prompts == 0)
                    active = true
                case 2:
                    precondition(prompts == 0)
                    presenting = false
                case 3:
                    precondition(prompts == 1 && !active)
                case 4:
                    precondition(prompts == 1)
                    active = true
                default: preconditionFailure("request did not finish")
                }
                await Task.yield()
            }
        )
        await request.resolve()
        precondition(prompts == 2 && pauses == 4)
        await request.resolve()
        precondition(prompts == 2, "resolved requests must not repeat")
    }

    static func coalescesConcurrentRequests() async {
        var determined = false
        var prompts = 0
        var releasePrompt: CheckedContinuation<Void, Never>?
        let request = TrackingRequestCoordinator(
            isDetermined: { determined },
            canPresent: { true },
            prompt: {
                prompts += 1
                await withCheckedContinuation { releasePrompt = $0 }
                determined = true
            }
        )
        let first = Task { await request.resolve() }
        while releasePrompt == nil { await Task.yield() }
        var joined = 0
        let others = (0..<20).map { _ in
            Task {
                joined += 1
                await request.resolve()
            }
        }
        while joined != others.count { await Task.yield() }
        precondition(prompts == 1, "only one system sheet may be pending")
        releasePrompt?.resume()
        await first.value
        for other in others { await other.value }
        precondition(prompts == 1)
    }
}
