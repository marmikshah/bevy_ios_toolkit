import XCTest
import Vision

/// Drive the installed Bevy demo through its rendered controls. Bevy draws
/// these labels into Metal, so OCR locates them instead of accessibility ids.
final class DemoSmoke: XCTestCase {
    struct Label {
        let text: String
        let bounds: CGRect
    }

    func recognize(_ data: Data, regions: [CGRect]) throws -> [Label] {
        let requests = regions.map { region in
            let request = VNRecognizeTextRequest()
            request.recognitionLevel = .accurate
            request.recognitionLanguages = ["en-US"] // the demo's labels are English
            request.regionOfInterest = region
            return request
        }
        try VNImageRequestHandler(data: data).perform(requests)
        return requests.flatMap { request in
            (request.results ?? []).compactMap { result in
                result.topCandidates(1).first.map { Label(text: $0.string, bounds: result.boundingBox) }
            }
        }
    }

    func labels() throws -> [Label] {
        try recognize(XCUIScreen.main.screenshot().pngRepresentation,
                      regions: [CGRect(x: 0, y: 0, width: 1, height: 1)])
    }

    func statusText(in app: XCUIApplication) throws -> String {
        var regions = [CGRect(x: 0, y: 0, width: 1, height: 1)]
        if app.frame.width > 600 {
            // Read long iPad status lines in shorter regions for OCR.
            regions += [CGRect(x: 0, y: 0.75, width: 0.5, height: 0.25),
                        CGRect(x: 0.5, y: 0.75, width: 0.5, height: 0.25)]
        }
        return try recognize(XCUIScreen.main.screenshot().pngRepresentation, regions: regions)
            .map(\.text).joined(separator: " ")
    }

    func waitFor(_ text: String, in app: XCUIApplication) throws -> Label {
        var previous: CGRect?
        for attempt in 0..<30 {
            let visible = try labels()
            if attempt == 0 {
                print("Looking for '\(text)': \(visible.map(\.text).joined(separator: " | "))")
            }
            if let label = visible.first(where: {
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
        let visible = try labels().map(\.text).joined(separator: " | ")
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
            visible = try statusText(in: app)
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

    func resolveConsent(in app: XCUIApplication) throws {
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
            let visible = try labels()
            if containsStatus("ads-ready: true", in: try statusText(in: app)) {
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
        capture("consent-unresolved")
        _ = try waitForStatus("ads-ready: true", in: app)
    }

    func capture(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
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
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 30))
        _ = try waitFor("Toggle Banner", in: app)
        _ = try waitForStatus("store: pending", in: app) // no explicit environment request
        XCTAssertFalse(app.alerts.firstMatch.exists)
        capture("launch")
        try resolveConsent(in: app)

        try tap("Toggle Banner", in: app)
        let banner = try waitForStatus("banner: true", in: app)
        XCTAssertNotNil(banner.range(of: #"[1-9][0-9]*\s*pt"#, options: .regularExpression))
        capture("banner")
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
        capture("foreground")
    }
}
