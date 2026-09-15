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
        for _ in 0..<30 {
            if let label = try labels(in: app).first(where: {
                $0.text.localizedCaseInsensitiveContains(text)
            }) { return label }
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
        for _ in 0..<30 {
            let visible = try labels(in: app).map(\.text).joined(separator: " ")
            if visible.localizedCaseInsensitiveContains(text) { return visible }
            Thread.sleep(forTimeInterval: 1)
        }
        XCTFail("Missing rendered status '\(text)'")
        throw NSError(domain: "DemoSmoke", code: 2)
    }

    func capture(_ name: String, app: XCUIApplication) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    func testLaunchBannerAndForeground() throws {
        continueAfterFailure = false
        let app = XCUIApplication(bundleIdentifier: "com.marmikshah.playground")
        app.launchEnvironment["BEVY_IOS_TOOLKIT_DEMO_QA"] = "1"
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 30))
        _ = try waitFor("Toggle Banner", in: app)
        _ = try waitFor("pending", in: app) // no explicit environment request
        XCTAssertFalse(app.alerts.firstMatch.exists)
        capture("launch", app: app)
        _ = try waitForStatus("ads-ready: true", in: app)

        try tap("Toggle Banner", in: app)
        let banner = try waitForStatus("banner: true", in: app)
        XCTAssertNotNil(banner.range(of: #"[1-9][0-9]*pt"#, options: .regularExpression))
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
