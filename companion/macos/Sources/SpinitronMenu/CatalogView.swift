import AppKit
import SwiftUI
import SpinitronCore

private struct DayGroup: Identifiable {
    let id: Date
    let playlists: [Playlist]
}

struct CatalogView: View {
    @ObservedObject var store: CatalogStore
    let showSettings: () -> Void
    let close: () -> Void
    @AppStorage("defaultStation") private var defaultStation = "KALX"
    @AppStorage("includeLegacy") private var includeLegacy = false
    @State private var station = UserDefaults.standard.string(forKey: "defaultStation") ?? "KALX"
    @State private var query = ""
    @FocusState private var searching: Bool

    private var matches: [Playlist] {
        Playlist.filtered(store.snapshot?.playlists ?? [], station: station, query: query, includeLegacy: includeLegacy)
    }

    private var groups: [DayGroup] {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "America/Los_Angeles")!
        return Dictionary(grouping: matches) { playlist in
            playlist.date.map { calendar.startOfDay(for: $0) } ?? .distantPast
        }.map { DayGroup(id: $0.key, playlists: $0.value) }.sorted { $0.id > $1.id }
    }

    var body: some View {
        VStack(spacing: 8) {
            titleBar
            stationControls
            searchControls
            if let error = store.error { errorBanner(error) }
            catalog
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(HiFi.paper)
                .modifier(HiFiBezel(radius: 3, recessed: true))
            footer
        }
        .padding(10)
        .background { SilverSurface() }
        .modifier(HiFiBezel(radius: 9))
        .padding(4)
        .background(HiFi.edge)
        .frame(width: 520, height: 640)
        .foregroundStyle(HiFi.ink)
        .tint(HiFi.blue)
        .preferredColorScheme(.light)
        .task { await store.start() }
        .onChange(of: defaultStation) { _, value in station = value }
        .background {
            Button("Find") { searching = true }.keyboardShortcut("f").hidden()
        }
    }

    private var titleBar: some View {
        HStack(spacing: 7) {
            DotMatrixText(text: "SPINITRON", pitch: 1.65)
                .padding(.leading, 4)
                .shadow(color: .white.opacity(0.9), radius: 0, x: 0, y: 1)
            Spacer()
            Button(action: showSettings) { Image(systemName: "gearshape") }
                .buttonStyle(HiFiButtonStyle(compact: true))
                .help("Settings").accessibilityLabel("Settings")
            Button(action: close) { Image(systemName: "xmark") }
                .buttonStyle(HiFiButtonStyle(compact: true))
                .help("Close panel").accessibilityLabel("Close panel")
        }
    }

    private var stationControls: some View {
        HStack(spacing: 10) {
            StationDisplay(station: station.isEmpty ? "ALL" : station)
            Menu {
                Button("All stations") { station = "" }
                Divider()
                ForEach(store.stations, id: \.self) { value in
                    Button(value) { station = value }
                }
            } label: {
                Image(systemName: "chevron.down").font(.system(size: 10, weight: .bold))
            }
            .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
            .frame(width: 24, height: HiFi.stationControlHeight)
            .modifier(HiFiButtonSurface())
            .help("All stations").accessibilityLabel("Choose station")
        }
    }

    private var searchControls: some View {
        HStack(spacing: 8) {
            HStack(spacing: 7) {
                Image(systemName: "magnifyingglass").foregroundStyle(HiFi.secondary)
                TextField("", text: $query)
                    .font(.system(size: 12))
                    .textFieldStyle(.plain).focused($searching)
                    .overlay(alignment: .leading) {
                        if query.isEmpty {
                            Text("Search shows, artists or songs")
                                .font(.system(size: 12)).foregroundStyle(HiFi.secondary)
                                .allowsHitTesting(false).accessibilityHidden(true)
                        }
                    }
                    .accessibilityLabel("Search shows, artists or songs")
                    .accessibilityIdentifier("catalog-search")
                if !query.isEmpty {
                    Button { query = ""; searching = true } label: { Image(systemName: "xmark.circle.fill") }
                        .buttonStyle(.plain).foregroundStyle(HiFi.secondary)
                        .help("Clear search").accessibilityLabel("Clear search")
                }
            }
            .padding(.horizontal, 10).frame(height: 33)
            .background(Color.white.opacity(0.67))
            .modifier(HiFiBezel(radius: 3, recessed: true))
            .overlay {
                if searching {
                    RoundedRectangle(cornerRadius: 3).strokeBorder(HiFi.blue, lineWidth: 2)
                        .allowsHitTesting(false)
                }
            }
            Button { Task { await store.refresh() } } label: {
                ZStack {
                    Image(systemName: "arrow.clockwise").opacity(store.isRefreshing ? 0 : 1)
                    if store.isRefreshing { ProgressView().controlSize(.small) }
                }.frame(width: 18)
            }
            .buttonStyle(HiFiButtonStyle()).disabled(store.isRefreshing)
            .keyboardShortcut("r")
            .help("Download the latest catalog (⌘R)")
            .accessibilityLabel(store.isRefreshing ? "Refreshing catalog" : "Refresh")
        }
    }

    @ViewBuilder private var catalog: some View {
        if store.snapshot == nil {
            VStack(spacing: 12) {
                if store.isRefreshing {
                    ProgressView().controlSize(.small)
                    Text("Loading playlists…")
                } else {
                    Image(systemName: "antenna.radiowaves.left.and.right").font(.system(size: 25))
                    Text("Refresh to load the catalog")
                    Button("Refresh") { Task { await store.refresh() } }.buttonStyle(HiFiButtonStyle())
                }
            }.font(.system(size: 13)).foregroundStyle(HiFi.secondary)
        } else if matches.isEmpty {
            VStack(spacing: 10) {
                Image(systemName: "magnifyingglass").font(.system(size: 25))
                Text("No matching playlists").font(.system(size: 14, weight: .semibold))
                Text("Try another search or station.\nOlder collections are available in Settings.")
                    .font(.system(size: 12)).multilineTextAlignment(.center)
            }.padding(24).foregroundStyle(HiFi.secondary)
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0, pinnedViews: .sectionHeaders) {
                    ForEach(groups) { group in
                        Section {
                            ForEach(group.playlists) { playlist in
                                PlaylistRow(playlist: playlist, showStation: station.isEmpty) { store.open(playlist) }
                            }
                        } header: { dayHeader(group) }
                    }
                }
            }
            .scrollIndicators(.visible)
        }
    }

    private func dayHeader(_ group: DayGroup) -> some View {
        VStack(spacing: 0) {
            HStack {
                Text(dayLabel(group.id))
                Spacer()
                Text("\(group.playlists.count)").monospacedDigit()
            }
            .font(.system(size: 10, weight: .semibold, design: .monospaced))
            .tracking(0.6)
            .padding(.horizontal, 11).padding(.vertical, 8)
            .background { SilverSurface() }
            HiFiRule()
        }
        .accessibilityAddTraits(.isHeader)
    }

    private var footer: some View {
        HStack(spacing: 8) {
            Image(systemName: "internaldrive").font(.system(size: 10))
            if let snapshot = store.snapshot {
                Text("\(matches.count) playlists · Refreshed \(snapshot.fetchedAt, style: .relative) ago")
                    .help(snapshot.fetchedAt.formatted(date: .abbreviated, time: .shortened))
                    .lineLimit(1)
            } else { Text("No saved catalog") }
            Spacer(minLength: 0)
            Button("Quit") { NSApplication.shared.terminate(nil) }
                .buttonStyle(HiFiButtonStyle(compact: true)).keyboardShortcut("q")
        }
        .font(.system(size: 10)).foregroundStyle(HiFi.secondary)
    }

    private func errorBanner(_ error: String) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: "exclamationmark.triangle")
            Text(error).font(.system(size: 11)).textSelection(.enabled)
            Spacer(minLength: 0)
            Button { store.error = nil } label: { Image(systemName: "xmark") }
                .buttonStyle(.plain).accessibilityLabel("Dismiss error")
        }
        .foregroundStyle(Color(red: 0.36, green: 0.20, blue: 0.06))
        .padding(10).background(Color(red: 1, green: 0.94, blue: 0.80))
        .modifier(HiFiBezel(radius: 3))
    }

    private func dayLabel(_ date: Date) -> String {
        if date == .distantPast { return "DATE UNAVAILABLE" }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "America/Los_Angeles")
        formatter.dateFormat = "EEE, MMM d · yyyy"
        return formatter.string(from: date).uppercased()
    }
}

struct PlaylistRow: View {
    let playlist: Playlist
    let showStation: Bool
    let open: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: open) {
            HStack(spacing: 10) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(playlist.browseTitle)
                        .font(.system(size: 13, weight: .semibold)).lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                    if !playlist.artistSample.isEmpty {
                        Text(playlist.artistSample).font(.system(size: 11))
                            .foregroundStyle(HiFi.secondary).lineLimit(1)
                    }
                    if showStation || !playlist.isBroadcast {
                        HStack(spacing: 6) {
                            if showStation { Text(playlist.station) }
                            if !playlist.isBroadcast { Text("Older collection") }
                        }.font(.system(size: 9, weight: .medium)).foregroundStyle(HiFi.secondary)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                Text("\(playlist.trackCount) tracks")
                    .font(.system(size: 10)).monospacedDigit().foregroundStyle(HiFi.secondary)
                    .fixedSize()
                Image(systemName: "arrow.up.right")
                    .font(.system(size: 14, weight: .medium)).foregroundStyle(HiFi.blue)
                    .frame(width: 17)
            }
            .padding(.horizontal, 12).padding(.vertical, 10)
            .frame(maxWidth: .infinity, minHeight: 58, alignment: .leading)
            .contentShape(Rectangle())
            .background(hovered ? HiFi.highlight : .clear)
            .overlay(alignment: .bottom) { HiFiRule() }
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .help("Open \(playlist.station) — \(playlist.title) in Spotify")
        .accessibilityLabel("Open \(playlist.station) — \(playlist.title) in Spotify")
    }
}

struct SettingsView: View {
    @ObservedObject var store: CatalogStore
    @AppStorage("defaultStation") private var defaultStation = "KALX"
    @AppStorage("includeLegacy") private var includeLegacy = false

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                DotMatrixText(text: "SPINITRON", pitch: 1.65)
                Spacer()
                Text("SETTINGS").font(.system(size: 10, weight: .semibold, design: .monospaced))
            }
            HiFiRule()
            VStack(alignment: .leading, spacing: 18) {
                Picker("Default station", selection: $defaultStation) {
                    Text("All stations").tag("")
                    ForEach(store.stations, id: \.self) { Text($0).tag($0) }
                }
                Toggle("Include older playlist collections", isOn: $includeLegacy)
                    .toggleStyle(.checkbox)
            }
            .font(.system(size: 13))
            Text("The catalog is saved on this Mac. Use Refresh to download new playlists. Broadcast dates use Pacific time.")
                .font(.system(size: 11)).foregroundStyle(HiFi.secondary).lineSpacing(3)
            Spacer(minLength: 0)
            HStack(spacing: 6) {
                Image(systemName: "paintpalette")
                Text("Silver Hi-Fi").font(.system(size: 10, weight: .semibold, design: .monospaced))
            }.foregroundStyle(HiFi.secondary)
        }
        .padding(22).frame(width: 420, height: 290)
        .foregroundStyle(HiFi.ink).tint(HiFi.blue)
        .background { SilverSurface() }
        .preferredColorScheme(.light)
        .task { await store.start() }
    }
}
