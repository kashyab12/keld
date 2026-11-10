// Original UI checks. XCTest drives the app externally; no in-app action replay.
import XCTest

final class InteractionTests: XCTestCase {
    override func setUpWithError() throws { continueAfterFailure = false }

    private func capture(_ app: XCUIApplication, _ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    private func openNote(_ app: XCUIApplication) {
        let search = app.textFields["search-items"]
        XCTAssertTrue(search.waitForExistence(timeout: 10))
        search.tap()
        search.typeText("Item 42\n")
        let result = app.tables.cells["Item 42"]
        XCTAssertTrue(result.waitForExistence(timeout: 5))
        result.tap()
        XCTAssertTrue(app.textViews["note-editor"].waitForExistence(timeout: 5))
    }

    func testSearchEditPersistAndReset() {
        let app = XCUIApplication()
        app.launch()
        app.buttons["reset-notes"].tap()
        XCTAssertEqual(app.staticTexts["saved-count"].label, "Saved: 0")
        openNote(app)
        let editor = app.textViews["note-editor"]
        editor.tap()
        editor.typeText("Sable app check")
        app.buttons["save-note"].tap()
        XCTAssertEqual(app.staticTexts["save-status"].label, "Note saved")
        capture(app, "note-saved")
        app.terminate()
        app.launch()
        XCTAssertEqual(app.staticTexts["saved-count"].label, "Saved: 1")
        openNote(app)
        XCTAssertEqual(app.textViews["note-editor"].value as? String, "Sable app check")
        capture(app, "note-persisted-after-process-relaunch")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.buttons["reset-notes"].tap()
        XCTAssertEqual(app.staticTexts["saved-count"].label, "Saved: 0")
        app.tables.cells["Item 42"].tap()
        XCTAssertEqual(app.textViews["note-editor"].value as? String, "")
        capture(app, "note-reset")
    }

    func testScrollAndNavigate() {
        let app = XCUIApplication()
        app.launch()
        let table = app.tables["items-table"]
        XCTAssertTrue(table.waitForExistence(timeout: 10))
        for _ in 0..<10 {
            if table.cells["Item 59"].isHittable { break }
            table.swipeUp()
        }
        XCTAssertTrue(table.cells["Item 59"].isHittable)
        table.cells["Item 59"].tap()
        XCTAssertTrue(app.navigationBars["Item 59"].waitForExistence(timeout: 5))
        capture(app, "scroll-to-last-item")
    }

    // Assert the composed, on-screen drawable too, rather than trusting a status label.
    private func assertCanvas(_ app: XCUIApplication, red: Bool) {
        let canvas = app.otherElements["metal-canvas"]
        XCTAssertTrue(canvas.waitForExistence(timeout: 5))
        let image = app.screenshot().image
        let frame = canvas.frame
        let rect = CGRect(x: frame.midX * image.scale, y: frame.midY * image.scale, width: 1, height: 1)
        guard let pixel = image.cgImage?.cropping(to: rect),
              let colorSpace = CGColorSpace(name: CGColorSpace.sRGB) else { XCTFail("No screenshot pixel"); return }
        var rgba = [UInt8](repeating: 0, count: 4)
        rgba.withUnsafeMutableBytes { bytes in
            let context = CGContext(data: bytes.baseAddress, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
                                    space: colorSpace, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
            context.draw(pixel, in: CGRect(x: 0, y: 0, width: 1, height: 1))
        }
        XCTAssertGreaterThan(rgba[red ? 0 : 2], 240)
        XCTAssertLessThan(rgba[red ? 2 : 0], 15)
        XCTAssertLessThan(rgba[1], 15)
    }

    func testMetalPresentedPixels() {
        let app = XCUIApplication()
        app.launch()
        app.buttons["open-metal"].tap()
        let status = app.staticTexts["gpu-status"]
        let red = NSPredicate(format: "label BEGINSWITH %@", "RGBA 255,0,0,255 | ")
        expectation(for: red, evaluatedWith: status)
        waitForExpectations(timeout: 15)
        let device = XCTAttachment(string: status.label)
        device.name = "metal-device-and-pixel-readback"
        device.lifetime = .keepAlways
        add(device)
        assertCanvas(app, red: true)
        capture(app, "metal-red-presented")
        app.buttons["toggle-color"].tap()
        expectation(for: NSPredicate(format: "label BEGINSWITH %@", "RGBA 0,0,255,255 | "), evaluatedWith: status)
        waitForExpectations(timeout: 15)
        assertCanvas(app, red: false)
        capture(app, "metal-blue-presented")
    }
}
