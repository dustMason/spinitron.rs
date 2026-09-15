import Foundation

public struct Track: Codable, Equatable, Sendable {
    public let name: String
    public let artists: [String]

    private enum CodingKeys: String, CodingKey { case name, artists }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        name = try values.decode(String.self, forKey: .name)
        if let artist = try? values.decode(String.self, forKey: .artists) {
            artists = [artist]
        } else {
            artists = try values.decode([String].self, forKey: .artists)
        }
    }
}

public struct Playlist: Codable, Identifiable, Equatable, Sendable {
    public let id: String
    public let station: String
    public let title: String
    public let date: Date?
    public let isBroadcast: Bool
    public let trackCount: Int
    public let preview: [Track]

    public var spotifyURL: URL { URL(string: "spotify:playlist:\(id)")! }
    public var artistSample: String {
        var seen = Set<String>()
        return preview.flatMap(\.artists).filter { seen.insert($0).inserted }
            .prefix(3).joined(separator: " · ")
    }

    public static func filtered(_ playlists: [Playlist], station: String, query: String,
                                includeLegacy: Bool) -> [Playlist] {
        let terms = query.split(whereSeparator: \.isWhitespace).map(String.init)
        return playlists.filter { playlist in
            guard (station.isEmpty || playlist.station == station),
                  includeLegacy || playlist.isBroadcast else { return false }
            let text = ([playlist.title, playlist.station] + playlist.preview.flatMap {
                [$0.name] + $0.artists
            }).joined(separator: " ")
            return terms.allSatisfy { text.range(of: $0, options: [.caseInsensitive, .diacriticInsensitive]) != nil }
        }.sorted {
            if $0.date != $1.date { return ($0.date ?? .distantPast) > ($1.date ?? .distantPast) }
            return $0.id < $1.id
        }
    }
}

public struct CatalogSnapshot: Codable, Sendable {
    public let version: Int
    public let fetchedAt: Date
    public let playlists: [Playlist]
}

public enum CatalogError: LocalizedError {
    case invalid(String)
    public var errorDescription: String? {
        if case .invalid(let message) = self { return message }
        return nil
    }
}

private struct FeedRow: Decodable {
    let station: String
    let name: String
    let display_name: String?
    let url: String
    let broadcast_start: String?
    let imported_at: String?
    let last_updated: String?
    let track_count: Int?
    let preview: [Track]?
}

public enum CatalogFeed {
    public static let url = URL(string: "https://fiftyfootfoghorn.com/spinitron.rs/playlists.jsonl")!

    public static func decode(_ data: Data, fetchedAt: Date = Date()) throws -> CatalogSnapshot {
        guard data.count <= 20_000_000, let text = String(data: data, encoding: .utf8) else {
            throw CatalogError.invalid("The catalog response is too large or is not valid text.")
        }
        let iso = ISO8601DateFormatter()
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let legacy = DateFormatter()
        legacy.locale = Locale(identifier: "en_US_POSIX")
        legacy.timeZone = TimeZone(secondsFromGMT: 0)
        legacy.dateFormat = "yyyy-MM-dd HH:mm 'UTC'"
        func date(_ value: String?) -> Date? {
            guard let value else { return nil }
            return fractional.date(from: value) ?? iso.date(from: value) ?? legacy.date(from: value)
        }
        var ids = Set<String>()
        let playlists = try text.split(whereSeparator: \.isNewline).enumerated().map { index, line in
            let row: FeedRow
            do { row = try JSONDecoder().decode(FeedRow.self, from: Data(line.utf8)) }
            catch { throw CatalogError.invalid("Invalid playlist on catalog line \(index + 1).") }
            guard let url = URLComponents(string: row.url), url.scheme == "https",
                  url.host == "open.spotify.com", url.user == nil, url.password == nil,
                  url.port == nil else { throw CatalogError.invalid("Invalid Spotify playlist link.") }
            let parts = url.path.split(separator: "/")
            guard parts.count == 2, parts[0] == "playlist",
                  parts[1].range(of: "^[A-Za-z0-9]{22}$", options: .regularExpression) != nil else {
                throw CatalogError.invalid("Invalid Spotify playlist ID.")
            }
            let id = String(parts[1])
            guard ids.insert(id).inserted else { throw CatalogError.invalid("The catalog contains duplicate playlists.") }
            let broadcast = date(row.broadcast_start)
            if row.broadcast_start != nil && broadcast == nil {
                throw CatalogError.invalid("Invalid broadcast date on catalog line \(index + 1).")
            }
            let fullTitle = row.display_name ?? row.name
            let prefix = row.station + " - "
            let title = fullTitle.hasPrefix(prefix) ? String(fullTitle.dropFirst(prefix.count)) : fullTitle
            return Playlist(id: id, station: row.station, title: title,
                            date: broadcast ?? date(row.imported_at) ?? date(row.last_updated),
                            isBroadcast: broadcast != nil, trackCount: max(0, row.track_count ?? 0),
                            preview: row.preview ?? [])
        }
        guard !playlists.isEmpty else { throw CatalogError.invalid("The catalog is empty. Keeping your saved list.") }
        return CatalogSnapshot(version: 1, fetchedAt: fetchedAt, playlists: playlists)
    }

    public static func download() async throws -> Data {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 30
        config.timeoutIntervalForResource = 45
        let session = URLSession(configuration: config)
        defer { session.finishTasksAndInvalidate() }
        let request = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData)
        let (data, response) = try await session.data(for: request)
        guard let response = response as? HTTPURLResponse, response.statusCode == 200 else {
            throw CatalogError.invalid("The catalog server is unavailable. Try Refresh again later.")
        }
        return data
    }
}

public actor CatalogRepository {
    private let cacheURL: URL
    private let download: @Sendable () async throws -> Data

    public init(cacheURL: URL, download: @escaping @Sendable () async throws -> Data = CatalogFeed.download) {
        self.cacheURL = cacheURL
        self.download = download
    }

    public func cached() throws -> CatalogSnapshot? {
        guard FileManager.default.fileExists(atPath: cacheURL.path) else { return nil }
        let snapshot = try JSONDecoder().decode(CatalogSnapshot.self, from: Data(contentsOf: cacheURL))
        guard snapshot.version == 1, !snapshot.playlists.isEmpty,
              snapshot.playlists.allSatisfy({ $0.id.range(of: "^[A-Za-z0-9]{22}$", options: .regularExpression) != nil }) else {
            throw CatalogError.invalid("The saved catalog needs to be refreshed.")
        }
        return snapshot
    }

    public func refresh() async throws -> CatalogSnapshot {
        let snapshot = try CatalogFeed.decode(await download())
        let data = try JSONEncoder().encode(snapshot)
        try FileManager.default.createDirectory(at: cacheURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        try data.write(to: cacheURL, options: .atomic)
        return snapshot
    }
}
