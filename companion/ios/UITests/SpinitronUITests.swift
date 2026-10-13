import XCTest

final class SpinitronUITests: XCTestCase {
    @MainActor private func app(reset: Bool = true, offline: Bool = false, largeText: Bool = false) throws -> XCUIApplication {
        continueAfterFailure = false
        let app = XCUIApplication()
        let fixture = try Data(contentsOf: XCTUnwrap(Bundle(for: Self.self).url(forResource: "catalog", withExtension: "jsonl")))
        app.launchEnvironment["SPINITRON_TEST_FEED"] = fixture.base64EncodedString()
        app.launchArguments = ["--ui-testing"]
        if reset { app.launchArguments.append("--reset-test-state") }
        if offline { app.launchArguments.append("--offline-test") }
        if largeText { app.launchArguments += ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryAccessibilityXXXL"] }
        app.launch()
        XCTAssertTrue(app.buttons["station-picker"].waitForExistence(timeout: 15))
        return app
    }

    @MainActor private func capture(_ name: String) {
        let image = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        image.name = name
        image.lifetime = .keepAlways
        add(image)
    }

    @MainActor func testBrowseSearchStationAndSpotifyFallback() throws {
        XCUIDevice.shared.orientation = .portrait
        let app = try app()
        let first = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'playlist-'")).firstMatch
        XCTAssertTrue(first.waitForExistence(timeout: 15))
        capture("iphone-portrait")
        first.tap()
        XCTAssertTrue(app.alerts["Spotify isn’t available"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.alerts.buttons["Open in browser"].exists)
        capture("spotify-unavailable")
        app.alerts.buttons["Cancel"].tap()
        app.buttons["station-picker"].tap()
        app.buttons["KPOO"].tap()
        XCTAssertEqual(app.buttons["station-picker"].value as? String, "KPOO")
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH '3 playlists'")).firstMatch.waitForExistence(timeout: 5))
        capture("station-kpoo")
        let search = app.searchFields.firstMatch
        search.tap()
        search.typeText("no-such-show-zzzz")
        XCTAssertTrue(app.staticTexts["No matching playlists"].waitForExistence(timeout: 5))
        capture("empty-search")
    }

    @MainActor func testSettingsAndOfflineCache() throws {
        XCUIDevice.shared.orientation = .portrait
        var app = try app()
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'playlist-'")).firstMatch.waitForExistence(timeout: 15))
        app.buttons["settings"].tap()
        XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 5))
        capture("settings")
        app.buttons["done-settings"].tap()
        app.terminate()
        app = try self.app(reset: false, offline: true)
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'playlist-'")).firstMatch.waitForExistence(timeout: 5))
        app.buttons["refresh-catalog"].tap()
        XCTAssertTrue(app.buttons["Retry"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'playlist-'")).firstMatch.exists)
        capture("offline-refresh")
        let retry = app.buttons["Retry"]
        let dismiss = app.buttons["Dismiss"]
        for button in [retry, dismiss] {
            XCTAssertGreaterThanOrEqual(button.frame.width, 44)
            XCTAssertGreaterThanOrEqual(button.frame.height, 44)
        }
        retry.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.95)).tap()
        XCTAssertTrue(dismiss.waitForExistence(timeout: 5))
        dismiss.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.95)).tap()
        XCTAssertTrue(dismiss.waitForNonExistence(timeout: 5))
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'playlist-'")).firstMatch.exists)
    }

    @MainActor func testLandscapeAndAccessibilityText() throws {
        XCUIDevice.shared.orientation = .landscapeLeft
        var app = try app()
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'playlist-'")).firstMatch.waitForExistence(timeout: 15))
        capture("iphone-landscape")
        app.terminate()
        XCUIDevice.shared.orientation = .portrait
        app = try self.app(largeText: true)
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'playlist-'")).firstMatch.waitForExistence(timeout: 15))
        capture("iphone-accessibility")
        app.swipeUp()
        capture("iphone-accessibility-scrolled")
        app.buttons["settings"].tap()
        XCTAssertTrue(app.buttons["done-settings"].waitForExistence(timeout: 5))
        capture("settings-accessibility")
    }
}
