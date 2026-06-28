// Google AdMob bridge for bevy_google_admob.
//
// Design contract with the Rust side (bevy_google_admob::ads):
//   - Every entry point is @_cdecl C-ABI, called FROM Rust.
//   - AdMob's async, delegate-driven work runs in Tasks on the main actor;
//     results surface as a POLLED EVENT QUEUE drained once per frame, never as
//     callbacks into Rust — re-entrancy against winit's event loop is not safe.
//     This mirrors the NativeBridge.swift / StoreKitBridge.swift pattern.
//   - admob_drain_events returns a pointer to a buffer owned here, valid only
//     until the next drain; Rust copies immediately.
//   - This file must COMPILE AND LINK even where the SDK is unavailable:
//     everything GoogleMobileAds-specific sits behind
//     #if canImport(GoogleMobileAds), with linking stubs otherwise.
//
// Integration (per game):
//   1. Add the Google Mobile Ads SDK via SPM: https://github.com/googleads/swift-package-manager-google-mobile-ads
//      (targets GoogleMobileAds v12.x and GoogleUserMessagingPlatform v2.x —
//      the "no GAD/UMP prefix" API. iOS 26 deployment needs no availability guards.)
//   2. Add this file to the app's Xcode target alongside NativeBridge.swift.
//   3. Set `GADApplicationIdentifier` in Info.plist to your AdMob app id (use
//      bevy_google_admob::ads::TEST_APP_ID while developing), and add the
//      `SKAdNetworkItems` Google ships for attribution.
//   4. Insert AdmobConfig with your ad unit ids. Symbol prefix `admob_` won't
//      collide with the game's own bridge.
//
// All shared state (the event queue + cached consent status) lives behind an
// OSAllocatedUnfairLock so the async SDK callbacks and the synchronous C-ABI
// getters stay race-free and Swift-6 clean. Ad objects themselves are touched
// only on the main actor.

import Foundation
import os

enum AdFormat: Int32 {
    case banner = 0
    case interstitial = 1
    case rewarded = 2
    case rewardedInterstitial = 3
    case appOpen = 4
}

#if canImport(GoogleMobileAds)
import GoogleMobileAds
#if canImport(UIKit)
import UIKit
#endif
#if canImport(UserMessagingPlatform)
import UserMessagingPlatform
#endif

private struct AdMobSharedState {
    var events: [[String: Any]] = []
    var consentStatus: Int32 = 0          // 0 unknown,1 required,2 not-required,3 obtained
    var eventsJSONPtr: UnsafeMutablePointer<CChar>?
}

final class AdMobBridge: NSObject, @unchecked Sendable {
    static let shared = AdMobBridge()

    private let shared_ = OSAllocatedUnfairLock(initialState: AdMobSharedState())

    // Touched only on the main actor.
    @MainActor private var interstitial: InterstitialAd?
    @MainActor private var rewarded: RewardedAd?
    @MainActor private var rewardedInterstitial: RewardedInterstitialAd?
    @MainActor private var appOpen: AppOpenAd?
    @MainActor private var banner: BannerView?
    @MainActor private var fullScreenDelegates: [AdFormat: FullScreenPresenter] = [:]
    @MainActor private var bannerDelegate: BannerObserver?

    // MARK: Event queue (thread-safe)

    func emit(_ format: AdFormat, _ kind: String, error: String = "", rewardAmount: Int = 0, rewardType: String = "") {
        var ev: [String: Any] = ["format": format.rawValue, "kind": kind]
        if !error.isEmpty { ev["error"] = error }
        if rewardAmount != 0 { ev["reward_amount"] = rewardAmount }
        if !rewardType.isEmpty { ev["reward_type"] = rewardType }
        shared_.withLock { $0.events.append(ev) }
    }

    private func setConsentStatus(_ value: Int32) {
        shared_.withLock { $0.consentStatus = value }
    }

    func consentStatusValue() -> Int32 { shared_.withLock { $0.consentStatus } }

    func drainEvents() -> UnsafePointer<CChar>? {
        shared_.withLockUnchecked { s in
            let json = (try? JSONSerialization.data(withJSONObject: s.events))
                .flatMap { String(data: $0, encoding: .utf8) } ?? "[]"
            s.events.removeAll(keepingCapacity: true)
            if let old = s.eventsJSONPtr { free(old) }
            s.eventsJSONPtr = strdup(json)
            return s.eventsJSONPtr.map { UnsafePointer($0) }
        }
    }

    // MARK: Lifecycle

    @MainActor
    func start(testDevices: [String]) {
        if !testDevices.isEmpty {
            MobileAds.shared.requestConfiguration.testDeviceIdentifiers = testDevices
        }
        MobileAds.shared.start(completionHandler: nil)
        refreshConsentInfo(present: false)
    }

    // MARK: Consent (UMP)

    @MainActor
    func requestConsent() { refreshConsentInfo(present: true) }

    @MainActor
    private func refreshConsentInfo(present: Bool) {
        #if canImport(UserMessagingPlatform)
        let params = UMPRequestParameters()
        UMPConsentInformation.sharedInstance.requestConsentInfoUpdate(with: params) { [weak self] error in
            guard let self else { return }
            if let error {
                NSLog("[admob] consent info update failed: %@", String(describing: error))
            }
            Task { @MainActor in
                self.cacheConsentStatus()
                if present, let vc = self.rootViewController() {
                    UMPConsentForm.loadAndPresentIfRequired(from: vc) { [weak self] _ in
                        Task { @MainActor in self?.cacheConsentStatus() }
                    }
                }
            }
        }
        #else
        setConsentStatus(3) // no UMP: treat as obtained
        #endif
    }

    @MainActor
    private func cacheConsentStatus() {
        #if canImport(UserMessagingPlatform)
        let value: Int32
        switch UMPConsentInformation.sharedInstance.consentStatus {
        case .required: value = 1
        case .notRequired: value = 2
        case .obtained: value = 3
        default: value = 0
        }
        setConsentStatus(value)
        #endif
    }

    // MARK: Loading

    @MainActor
    func load(_ format: AdFormat, unitID: String) {
        Task { @MainActor in
            do {
                switch format {
                case .interstitial:
                    let ad = try await InterstitialAd.load(with: unitID, request: Request())
                    ad.fullScreenContentDelegate = self.delegate(for: .interstitial)
                    self.interstitial = ad
                case .rewarded:
                    let ad = try await RewardedAd.load(with: unitID, request: Request())
                    ad.fullScreenContentDelegate = self.delegate(for: .rewarded)
                    self.rewarded = ad
                case .rewardedInterstitial:
                    let ad = try await RewardedInterstitialAd.load(with: unitID, request: Request())
                    ad.fullScreenContentDelegate = self.delegate(for: .rewardedInterstitial)
                    self.rewardedInterstitial = ad
                case .appOpen:
                    let ad = try await AppOpenAd.load(with: unitID, request: Request())
                    ad.fullScreenContentDelegate = self.delegate(for: .appOpen)
                    self.appOpen = ad
                case .banner:
                    return // banners load via showBanner
                }
                self.emit(format, "loaded")
            } catch {
                self.emit(format, "load_failed", error: String(describing: error))
            }
        }
    }

    // MARK: Presenting

    @MainActor
    func show(_ format: AdFormat) {
        guard let vc = rootViewController() else {
            emit(format, "show_failed", error: "no root view controller")
            return
        }
        switch format {
        case .interstitial:
            guard let ad = interstitial else { return emit(format, "show_failed", error: "ad not loaded") }
            ad.present(from: vc)
        case .appOpen:
            guard let ad = appOpen else { return emit(format, "show_failed", error: "ad not loaded") }
            ad.present(from: vc)
        case .rewarded:
            guard let ad = rewarded else { return emit(format, "show_failed", error: "ad not loaded") }
            ad.present(from: vc) { [weak self, weak ad] in
                guard let ad else { return }
                self?.emit(.rewarded, "reward", rewardAmount: ad.adReward.amount.intValue, rewardType: ad.adReward.type)
            }
        case .rewardedInterstitial:
            guard let ad = rewardedInterstitial else { return emit(format, "show_failed", error: "ad not loaded") }
            ad.present(from: vc) { [weak self, weak ad] in
                guard let ad else { return }
                self?.emit(.rewardedInterstitial, "reward", rewardAmount: ad.adReward.amount.intValue, rewardType: ad.adReward.type)
            }
        case .banner:
            return
        }
    }

    @MainActor
    func clear(_ format: AdFormat) {
        switch format {
        case .interstitial: interstitial = nil
        case .rewarded: rewarded = nil
        case .rewardedInterstitial: rewardedInterstitial = nil
        case .appOpen: appOpen = nil
        case .banner: break
        }
    }

    // MARK: Banner

    @MainActor
    func showBanner(unitID: String, position: Int32) {
        guard let vc = rootViewController(), let host = vc.view else {
            emit(.banner, "show_failed", error: "no root view")
            return
        }
        banner?.removeFromSuperview()
        let view = BannerView(adSize: AdSizeBanner)
        view.adUnitID = unitID
        view.rootViewController = vc
        let observer = BannerObserver(bridge: self)
        view.delegate = observer
        bannerDelegate = observer
        view.translatesAutoresizingMaskIntoConstraints = false
        host.addSubview(view)

        let guide = host.safeAreaLayoutGuide
        let vertical = position == 0
            ? view.topAnchor.constraint(equalTo: guide.topAnchor)
            : view.bottomAnchor.constraint(equalTo: guide.bottomAnchor)
        NSLayoutConstraint.activate([
            view.centerXAnchor.constraint(equalTo: host.centerXAnchor),
            vertical,
        ])

        view.load(Request())
        banner = view
    }

    @MainActor
    func hideBanner() {
        banner?.removeFromSuperview()
        banner = nil
        bannerDelegate = nil
    }

    // MARK: Helpers

    @MainActor
    private func delegate(for format: AdFormat) -> FullScreenPresenter {
        if let d = fullScreenDelegates[format] { return d }
        let d = FullScreenPresenter(format: format, bridge: self)
        fullScreenDelegates[format] = d
        return d
    }

    @MainActor
    private func rootViewController() -> UIViewController? {
        UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .flatMap { $0.windows }
            .first { $0.isKeyWindow }?
            .rootViewController
    }
}

// Per-format delegate for full-screen ads, retained by the bridge while a
// creative is alive. Maps SDK callbacks back to events tagged with the format.
@MainActor
private final class FullScreenPresenter: NSObject, FullScreenContentDelegate {
    let format: AdFormat
    unowned let bridge: AdMobBridge

    init(format: AdFormat, bridge: AdMobBridge) {
        self.format = format
        self.bridge = bridge
    }

    func adWillPresentFullScreenContent(_ ad: FullScreenPresentingAd) {
        bridge.emit(format, "shown")
    }
    func adDidRecordClick(_ ad: FullScreenPresentingAd) {
        bridge.emit(format, "clicked")
    }
    func ad(_ ad: FullScreenPresentingAd, didFailToPresentFullScreenContentWithError error: Error) {
        bridge.emit(format, "show_failed", error: String(describing: error))
        bridge.clear(format)
    }
    func adDidDismissFullScreenContent(_ ad: FullScreenPresentingAd) {
        bridge.emit(format, "dismissed")
        bridge.clear(format)
    }
}

@MainActor
private final class BannerObserver: NSObject, BannerViewDelegate {
    unowned let bridge: AdMobBridge
    init(bridge: AdMobBridge) { self.bridge = bridge }

    func bannerViewDidReceiveAd(_ bannerView: BannerView) {
        bridge.emit(.banner, "loaded")
        bridge.emit(.banner, "shown")
    }
    func bannerView(_ bannerView: BannerView, didFailToReceiveAdWithError error: Error) {
        bridge.emit(.banner, "load_failed", error: String(describing: error))
    }
}

@_cdecl("admob_init")
public func admob_init(_ testDevices: UnsafePointer<CChar>, _ useTestAds: Int32) {
    let list = String(cString: testDevices)
        .split(separator: ",")
        .map { $0.trimmingCharacters(in: .whitespaces) }
        .filter { !$0.isEmpty }
    Task { @MainActor in AdMobBridge.shared.start(testDevices: list) }
}

@_cdecl("admob_load")
public func admob_load(_ format: Int32, _ unitID: UnsafePointer<CChar>) {
    guard let f = AdFormat(rawValue: format) else { return }
    let id = String(cString: unitID)
    Task { @MainActor in AdMobBridge.shared.load(f, unitID: id) }
}

@_cdecl("admob_show")
public func admob_show(_ format: Int32) {
    guard let f = AdFormat(rawValue: format) else { return }
    Task { @MainActor in AdMobBridge.shared.show(f) }
}

@_cdecl("admob_banner_show")
public func admob_banner_show(_ unitID: UnsafePointer<CChar>, _ position: Int32) {
    let id = String(cString: unitID)
    Task { @MainActor in AdMobBridge.shared.showBanner(unitID: id, position: position) }
}

@_cdecl("admob_banner_hide")
public func admob_banner_hide() {
    Task { @MainActor in AdMobBridge.shared.hideBanner() }
}

@_cdecl("admob_request_consent")
public func admob_request_consent() {
    Task { @MainActor in AdMobBridge.shared.requestConsent() }
}

@_cdecl("admob_consent_status")
public func admob_consent_status() -> Int32 { AdMobBridge.shared.consentStatusValue() }

@_cdecl("admob_drain_events")
public func admob_drain_events() -> UnsafePointer<CChar>? { AdMobBridge.shared.drainEvents() }

#else
// GoogleMobileAds unavailable: linking stubs. Nothing loads; consent obtained.

@_cdecl("admob_init") public func admob_init(_ testDevices: UnsafePointer<CChar>, _ useTestAds: Int32) {}
@_cdecl("admob_load") public func admob_load(_ format: Int32, _ unitID: UnsafePointer<CChar>) {}
@_cdecl("admob_show") public func admob_show(_ format: Int32) {}
@_cdecl("admob_banner_show") public func admob_banner_show(_ unitID: UnsafePointer<CChar>, _ position: Int32) {}
@_cdecl("admob_banner_hide") public func admob_banner_hide() {}
@_cdecl("admob_request_consent") public func admob_request_consent() {}
@_cdecl("admob_consent_status") public func admob_consent_status() -> Int32 { 3 }
@_cdecl("admob_drain_events") public func admob_drain_events() -> UnsafePointer<CChar>? { nil }

#endif
