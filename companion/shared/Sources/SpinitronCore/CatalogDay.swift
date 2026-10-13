import Foundation

public struct CatalogDay: Identifiable, Sendable {
    public let id: Date
    public let playlists: [Playlist]

    public var label: String {
        guard id != .distantPast else { return "Date unavailable" }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "America/Los_Angeles")
        formatter.dateFormat = "EEE, MMM d · yyyy"
        return formatter.string(from: id)
    }

    /// Broadcast dates remain Pacific on both devices, even when traveling.
    public static func grouped(_ playlists: [Playlist]) -> [CatalogDay] {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "America/Los_Angeles")!
        return Dictionary(grouping: playlists) { playlist in
            playlist.date.map { calendar.startOfDay(for: $0) } ?? .distantPast
        }.map { CatalogDay(id: $0.key, playlists: $0.value) }.sorted { $0.id > $1.id }
    }
}
