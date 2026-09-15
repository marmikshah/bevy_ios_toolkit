import XCTest
import Vision

/// Drive the installed Bevy demo through its rendered controls. Bevy draws
/// these labels into Metal, so OCR locates them instead of accessibility ids.
final class DemoSmoke: XCTestCase {
    struct Label {
        let text: String
        let bounds: CGRect
    }

    func labels(in app: XCUIApplication) throws -> [Label] {
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        let handler = VNImageRequestHandler(cgImage: app.screenshot().image.cgImage!)
        try handler.perform([request])
        return (request.results ?? []).compactMap { result in
            result.topCandidates(1).first.map { Label(text: $0.string, bounds: result.boundingBox) }
        }
    }

    func waitFor(_ text: String, in app: XCUIApplication) throws -> Label {
        var previous: CGRect?
        for _ in 0..<30 {
            if let label = try labels(in: app).first(where: {
                $0.text.localizedCaseInsensitiveContains(text)
            }) {
                // Foreground state can arrive during SpringBoard's zoom
                // animation. Tap only after the rendered control settles.
                if let previous,
                   abs(previous.midX - label.bounds.midX) < 0.01,
                   abs(previous.midY - label.bounds.midY) < 0.01 {
                    return label
                }
                previous = label.bounds
            } else {
                previous = nil
            }
            Thread.sleep(forTimeInterval: 1)
        }
        let visible = try labels(in: app).map(\.text).joined(separator: " | ")
        XCTFail("Missing rendered text '\(text)'; saw: \(visible)")
        throw NSError(domain: "DemoSmoke", code: 1)
    }

    func tap(_ text: String, in app: XCUIApplication) throws {
        let label = try waitFor(text, in: app)
        app.coordinate(withNormalizedOffset: CGVector(
            dx: label.bounds.midX, dy: 1 - label.bounds.midY
        )).tap()
    }

    func waitForStatus(_ text: String, in app: XCUIApplication) throws -> String {
        var visible = ""
        for _ in 0..<30 {
            visible = try labels(in: app).map(\.text).joined(separator: " ")
            if containsStatus(text, in: visible) { return visible }
            Thread.sleep(forTimeInterval: 1)
        }
        XCTFail("Missing rendered status '\(text)'; saw: \(visible)")
        throw NSError(domain: "DemoSmoke", code: 2)
    }

    func containsStatus(_ text: String, in visible: String) -> Bool {
        // Vision splits wrapped labels, including words broken at a hyphen.
        let compact = visible.filter { !$0.isWhitespace }.lowercased()
        return compact.contains(text.filter { !$0.isWhitespace }.lowercased())
    }

    func resolveTestConsent(in app: XCUIApplication) throws {
        try tap("Request Ad Consent", in: app)
        for _ in 0..<30 {
            let denyTracking = app.alerts.buttons["Ask App Not to Track"]
            if denyTracking.exists {
                denyTracking.tap()
                continue
            }
            let systemDeny = XCUIApplication(bundleIdentifier: "com.apple.springboard")
                .alerts.buttons["Ask App Not to Track"]
            if systemDeny.exists {
                systemDeny.tap()
                continue
            }
            let visible = try labels(in: app)
            if containsStatus("ads-ready: true", in: visible.map(\.text).joined(separator: " ")) {
                return
            }
            // UMP can still present its sample form outside the EEA. Exercise
            // that native flow on this simulator with Google's test ad ids.
            for choice in ["Do not consent", "Reject all", "Consent", "I consent", "Continue"] {
                if let button = visible.first(where: { $0.text.lowercased() == choice.lowercased() }) {
                    app.coordinate(withNormalizedOffset: CGVector(
                        dx: button.bounds.midX, dy: 1 - button.bounds.midY
                    )).tap()
                    break
                }
            }
            Thread.sleep(forTimeInterval: 1)
        }
        capture("consent-unresolved", app: app)
        _ = try waitForStatus("ads-ready: true", in: app)
    }

    func capture(_ name: String, app: XCUIApplication) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func testLaunchBannerAndForeground() throws {
        continueAfterFailure = false
        addUIInterruptionMonitor(withDescription: "Tracking permission") { alert in
            let deny = alert.buttons["Ask App Not to Track"]
            guard deny.exists else { return false }
            deny.tap()
            return true
        }
        let app = XCUIApplication(bundleIdentifier: "com.marmikshah.playground")
        app.launchEnvironment["BEVY_IOS_TOOLKIT_DEMO_QA"] = "1"
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 30))
        _ = try waitFor("Toggle Banner", in: app)
        _ = try waitFor("pending", in: app) // no explicit environment request
        XCTAssertFalse(app.alerts.firstMatch.exists)
        capture("launch", app: app)
        try resolveTestConsent(in: app)

        try tap("Toggle Banner", in: app)
        let banner = try waitForStatus("banner: true", in: app)
        XCTAssertNotNil(banner.range(of: #"[1-9][0-9]*\s*pt"#, options: .regularExpression))
        capture("banner", app: app)
        try tap("Toggle Banner", in: app)
        _ = try waitForStatus("banner: false", in: app)

        XCUIDevice.shared.press(.home)
        XCTAssertTrue(app.wait(for: .runningBackground, timeout: 15))
        app.activate()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 15))
        try tap("Toggle Banner", in: app)
        _ = try waitForStatus("banner: true", in: app) // rendered state still advances
        try tap("Toggle Banner", in: app)
        _ = try waitForStatus("banner: false", in: app)
        capture("foreground", app: app)
    }
}
