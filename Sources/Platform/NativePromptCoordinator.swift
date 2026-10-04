import Foundation

/// Presentation ownership is separate from queued SDK work and game state.
/// Native integrations acquire this only when an active presenter is eligible.
@MainActor
public final class NativePromptCoordinator {
    public enum Kind: Equatable, Sendable {
        case tracking, consent, notifications, purchase, restore, advertisement
    }

    public struct Lease: Equatable, Sendable {
        fileprivate let id: UInt64
    }

    public static let shared = NativePromptCoordinator()
    private var nextID: UInt64 = 0
    private var lease: Lease?
    public private(set) var kind: Kind?

    public init() {}

    public var isIdle: Bool { lease == nil }

    public func tryBegin(_ kind: Kind) -> Lease? {
        guard isIdle else { return nil }
        nextID &+= 1
        let lease = Lease(id: nextID)
        self.lease = lease
        self.kind = kind
        return lease
    }

    /// Suspend an async task, never the Bevy or UIKit event loop. Cancellation
    /// abandons queued work; a completed owner must release its own lease.
    public func begin(
        _ kind: Kind,
        canPresent: () -> Bool,
        pause: () async -> Void = { try? await Task.sleep(for: .milliseconds(250)) }
    ) async -> Lease? {
        while !Task.isCancelled {
            if canPresent(), let lease = tryBegin(kind) { return lease }
            await pause()
        }
        return nil
    }

    public func end(_ lease: Lease) {
        guard self.lease == lease else { return }
        self.lease = nil
        kind = nil
    }
}
