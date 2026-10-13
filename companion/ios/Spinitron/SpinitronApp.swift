import SwiftUI
import SpinitronCore

@main
struct SpinitronIOSApp: App {
    @StateObject private var store: CatalogStore

    init() {
        #if DEBUG && targetEnvironment(simulator)
        let process = ProcessInfo.processInfo
        if process.arguments.contains("--ui-testing"),
           let encoded = process.environment["SPINITRON_TEST_FEED"],
           let fixture = Data(base64Encoded: encoded) {
            let cache = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("ui-tests/catalog.json")
            if process.arguments.contains("--reset-test-state") {
                try? FileManager.default.removeItem(at: cache)
                UserDefaults.standard.removeObject(forKey: "defaultStation")
                UserDefaults.standard.removeObject(forKey: "includeLegacy")
            }
            let offline = process.arguments.contains("--offline-test")
            _store = StateObject(wrappedValue: CatalogStore(repository: CatalogRepository(cacheURL: cache) {
                if offline { throw URLError(.notConnectedToInternet) }
                return fixture
            }))
            return
        }
        #endif
        _store = StateObject(wrappedValue: CatalogStore())
    }

    var body: some Scene {
        WindowGroup {
            CatalogView(store: store)
                // Silver Hi-Fi has an intentionally fixed light material palette,
                // matching the Mac companion rather than inverting its textures.
                .preferredColorScheme(.light)
        }
    }
}
