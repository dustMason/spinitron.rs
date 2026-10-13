import SwiftUI
import SpinitronCore

/// Native, accessible text; the surrounding button belongs to the app shell.
public struct PlaylistSummary: View {
    let playlist: Playlist
    let showStation: Bool
    let compact: Bool

    public init(playlist: Playlist, showStation: Bool, compact: Bool = false) {
        self.playlist = playlist
        self.showStation = showStation
        self.compact = compact
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: compact ? 4 : 5) {
            Text(playlist.browseTitle)
                .font(compact ? .system(size: 13, weight: .semibold) : .headline)
                .foregroundStyle(HiFi.ink)
                .lineLimit(compact ? 2 : nil)
                .fixedSize(horizontal: false, vertical: true)
            if !playlist.artistSample.isEmpty {
                Text(playlist.artistSample)
                    .font(compact ? .system(size: 11) : .subheadline)
                    .foregroundStyle(HiFi.secondary)
                    .lineLimit(compact ? 1 : nil)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if !compact || showStation || !playlist.isBroadcast {
                Text(metadata)
                    .font(compact ? .system(size: 9, weight: .medium) : .caption)
                    .foregroundStyle(HiFi.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var metadata: String {
        var parts = [String]()
        if showStation { parts.append(playlist.station) }
        if !playlist.isBroadcast { parts.append("Older collection") }
        if !compact { parts.append("\(playlist.trackCount) tracks") }
        return parts.joined(separator: " · ")
    }
}
