import Foundation
import Testing
@testable import SpinitronCore

private let firstID = "6GLaEvEBQC7otjMM98z9AD"
private let secondID = "3mvfdAw6rNVqUb2D3Xl6Ob"

private func row(id: String = firstID, station: String = "KALX", broadcast: String? = "2026-09-09T17:00:00-0700") -> [String: Any] {
    var result: [String: Any] = [
        "station": station, "name": "old name", "display_name": "\(station) - Radio Dunya - 2026-09-09",
        "url": "https://open.spotify.com/playlist/\(id)", "track_count": 10,
        "last_updated": "2026-09-15 11:15 UTC",
        "imported_at": "2026-09-15T11:15:29.185903659+00:00",
        "preview": [["name": "音楽 <live>", "artists": ["Gerardo Batiz", "Tino Contreras"]]],
    ]
    if let broadcast { result["broadcast_start"] = broadcast }
    return result
}

private func feed(_ rows: [[String: Any]]) throws -> Data {
    try rows.map { try JSONSerialization.data(withJSONObject: $0, options: [.sortedKeys]) }
        .reduce(into: Data()) { $0.append($1); $0.append(10) }
}

@Test func decodesBroadcastDatesTitlesAndSpotifyURI() throws {
    let snapshot = try CatalogFeed.decode(feed([row()]))
    let playlist = try #require(snapshot.playlists.first)
    #expect(playlist.title == "Radio Dunya - 2026-09-09")
    #expect(playlist.spotifyURL.absoluteString == "spotify:playlist:\(firstID)")
    #expect(playlist.date == ISO8601DateFormatter().date(from: "2026-09-10T00:00:00Z"))
    #expect(playlist.isBroadcast)
    #expect(playlist.artistSample == "Gerardo Batiz · Tino Contreras")
}

@Test func filtersStationAndAllSearchTermsAndHidesLegacyByDefault() throws {
    let items = try CatalogFeed.decode(feed([row(), row(id: secondID, station: "KPOO", broadcast: nil)])).playlists
    #expect(Playlist.filtered(items, station: "KALX", query: "DUNYA 音楽 batiz", includeLegacy: false).map(\.id) == [firstID])
    #expect(Playlist.filtered(items, station: "KPOO", query: "", includeLegacy: false).isEmpty)
    #expect(Playlist.filtered(items, station: "", query: "", includeLegacy: true).count == 2)
    #expect(Playlist.filtered(items, station: "", query: "missing", includeLegacy: true).isEmpty)
}

@Test func acceptsBothArtistFormatsAndKeepsIndividualNamesIntact() throws {
    var record = row()
    record["preview"] = [
        ["name": "Wildwood Flower", "artists": "The Carter Family"],
        ["name": "Another song", "artists": ["The Carter Family", "The Blue Sky Boys", "Floyd Tillman", "Fourth Artist"]],
    ]
    let playlist = try CatalogFeed.decode(feed([record])).playlists[0]
    #expect(playlist.preview[0].artists == ["The Carter Family"])
    #expect(playlist.artistSample == "The Carter Family · The Blue Sky Boys · Floyd Tillman")
    let cached = try JSONDecoder().decode(Playlist.self, from: JSONEncoder().encode(playlist))
    #expect(cached == playlist)
}

@Test func sortsByBroadcastInsteadOfImportAndAcceptsFractionalImportDates() throws {
    let items = try CatalogFeed.decode(feed([row(broadcast: "2026-09-02T17:00:00-0700"), row(id: secondID)])).playlists
    #expect(Playlist.filtered(items, station: "", query: "", includeLegacy: false).map(\.id) == [secondID, firstID])
    let legacy = try CatalogFeed.decode(feed([row(broadcast: nil)])).playlists[0]
    #expect(legacy.date != nil)
    #expect(!legacy.isBroadcast)
}

@Test func rejectsUnsafeURLsAndInvalidOrDuplicateRecords() throws {
    for url in ["spotify:playlist:\(firstID)", "https://evil.com/playlist/\(firstID)",
                "https://open.spotify.com@evil.com/playlist/\(firstID)",
                "https://open.spotify.com/track/\(firstID)", "https://open.spotify.com/playlist/invalid"] {
        var record = row()
        record["url"] = url
        let data = try feed([record])
        #expect(throws: CatalogError.self) { try CatalogFeed.decode(data) }
    }
    let duplicate = try feed([row(), row()])
    #expect(throws: CatalogError.self) { try CatalogFeed.decode(duplicate) }
    #expect(throws: CatalogError.self) { try CatalogFeed.decode(Data()) }
    let badDate = try feed([row(broadcast: "yesterday")])
    #expect(throws: CatalogError.self) { try CatalogFeed.decode(badDate) }
}

@Test func cachedCatalogLoadsOfflineWithoutDownloading() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let data = try feed([row()])
    let cache = root.appendingPathComponent("catalog.json")
    let repository = CatalogRepository(cacheURL: cache, download: { data })
    #expect(try await repository.cached() == nil)
    let fresh = try await repository.refresh()
    let offline = CatalogRepository(cacheURL: cache, download: { throw URLError(.notConnectedToInternet) })
    let cached = try #require(try await offline.cached())
    #expect(cached.playlists == fresh.playlists)
    #expect(cached.fetchedAt == fresh.fetchedAt)
}

@Test func failedOrMalformedRefreshPreservesLastGoodCache() async throws {
    let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: root) }
    let data = try feed([row()])
    let cache = root.appendingPathComponent("catalog.json")
    _ = try await CatalogRepository(cacheURL: cache, download: { data }).refresh()
    let before = try Data(contentsOf: cache)
    let malformed = CatalogRepository(cacheURL: cache, download: { Data("<html>error</html>".utf8) })
    await #expect(throws: CatalogError.self) { try await malformed.refresh() }
    #expect(try Data(contentsOf: cache) == before)
    let offline = CatalogRepository(cacheURL: cache, download: { throw URLError(.notConnectedToInternet) })
    await #expect(throws: URLError.self) { try await offline.refresh() }
    #expect(try Data(contentsOf: cache) == before)
}

@Test func readsDeployedCatalogWhenFixtureProvided() throws {
    guard let path = ProcessInfo.processInfo.environment["SPINITRON_CATALOG_FIXTURE"] else { return }
    let data = try Data(contentsOf: URL(fileURLWithPath: path))
    let snapshot = try CatalogFeed.decode(data)
    let lines = try #require(String(data: data, encoding: .utf8)).split(whereSeparator: \.isNewline)
    #expect(snapshot.playlists.count == lines.count)
    #expect(snapshot.playlists.contains { $0.station == "KALX" && $0.isBroadcast })
}
