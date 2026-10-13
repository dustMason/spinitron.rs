import SwiftUI
import UIKit
import SpinitronCore
import SpinitronUI

struct CatalogView: View {
    @ObservedObject var store: CatalogStore
    @AppStorage("defaultStation") private var defaultStation = "KALX"
    @AppStorage("includeLegacy") private var includeLegacy = false
    @State private var station = UserDefaults.standard.string(forKey: "defaultStation") ?? "KALX"
    @State private var query = ""
    @State private var settingsPresented = false
    @State private var stationPresented = false
    @State private var unavailablePlaylist: Playlist?
    @Environment(\.openURL) private var openURL
    @Environment(\.verticalSizeClass) private var verticalSizeClass
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    private var compactLayout: Bool { verticalSizeClass == .compact || dynamicTypeSize.isAccessibilitySize }

    private var matches: [Playlist] {
        Playlist.filtered(store.snapshot?.playlists ?? [], station: station,
                          query: query, includeLegacy: includeLegacy)
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                if !compactLayout {
                    stationPicker.padding(.horizontal, 12).padding(.vertical, 8)
                }
                if let error = store.error { refreshError(error) }
                catalog
                if !compactLayout { statusBar }
            }
            .background { SilverSurface().ignoresSafeArea() }
            .foregroundStyle(HiFi.ink)
            .tint(HiFi.blue)
            .navigationTitle("Spinitron")
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(HiFi.paper, for: .navigationBar)
            .toolbarBackground(.visible, for: .navigationBar)
            .toolbar { toolbar }
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always),
                        prompt: "Search shows, artists or songs")
            .autocorrectionDisabled()
            .textInputAutocapitalization(.never)
            .sheet(isPresented: $settingsPresented) { SettingsView(store: store) }
            .confirmationDialog("Choose station", isPresented: $stationPresented, titleVisibility: .visible) {
                Button("All stations") { station = "" }
                ForEach(store.stations, id: \.self) { value in
                    Button(value) { station = value }
                }
                Button("Cancel", role: .cancel) {}
            }
            .alert("Spotify isn’t available", isPresented: Binding(
                get: { unavailablePlaylist != nil },
                set: { if !$0 { unavailablePlaylist = nil } }
            ), presenting: unavailablePlaylist) { playlist in
                Button("Open in browser") { openURL(playlist.webURL) }
                Button("Cancel", role: .cancel) {}
            } message: { _ in
                Text("Install Spotify to open playlists in the app, or view this playlist in your browser.")
            }
            .task { await store.start() }
            .onChange(of: defaultStation) { _, value in station = value }
        }
    }

    @ToolbarContentBuilder private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .principal) {
            DotMatrixText(text: "SPINITRON", pitch: 2).foregroundStyle(HiFi.ink)
        }
        if #available(iOS 26, *) {
            ToolbarItem(placement: .topBarTrailing) { toolbarButtons }
                .sharedBackgroundVisibility(.hidden)
        } else {
            ToolbarItem(placement: .topBarTrailing) { toolbarButtons }
        }
    }

    private var toolbarButtons: some View {
        HStack(spacing: 8) {
            Button { Task { await store.refresh() } } label: {
                ZStack {
                    Image(systemName: "arrow.clockwise").opacity(store.isRefreshing ? 0 : 1)
                    if store.isRefreshing { ProgressView() }
                }
            }
            .buttonStyle(HiFiButtonStyle(compact: true, touch: true))
            .disabled(store.isRefreshing)
            .accessibilityLabel(store.isRefreshing ? "Refreshing catalog" : "Refresh catalog")
            .accessibilityIdentifier("refresh-catalog")
            Button { settingsPresented = true } label: { Image(systemName: "gearshape") }
                .buttonStyle(HiFiButtonStyle(compact: true, touch: true))
                .accessibilityLabel("Settings")
                .accessibilityIdentifier("settings")
        }
    }

    private var stationPicker: some View {
        Button { stationPresented = true } label: {
            StationDisplay(station: station.isEmpty ? "ALL" : station, height: compactLayout ? 48 : 64)
                .overlay(alignment: .trailing) {
                    Image(systemName: "chevron.down")
                        .font(.body.weight(.semibold)).padding(.trailing, 16)
                        .accessibilityHidden(true)
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Choose station")
        .accessibilityValue(station.isEmpty ? "All stations" : station)
        .accessibilityHint("Changes the station used for browsing and search")
        .accessibilityIdentifier("station-picker")
    }

    @ViewBuilder private var catalog: some View {
        if store.snapshot == nil {
            VStack(spacing: 16) {
                if store.isRefreshing {
                    ProgressView()
                    Text("Loading playlists…")
                } else {
                    ContentUnavailableView("Load your radio catalog", systemImage: "radio",
                                           description: Text("Refresh to download the broadcast playlists."))
                    Button("Refresh") { Task { await store.refresh() } }
                        .buttonStyle(HiFiButtonStyle(touch: true))
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            List {
                if compactLayout {
                    stationPicker
                        .listRowInsets(EdgeInsets(top: 8, leading: 12, bottom: 8, trailing: 12))
                        .listRowSeparator(.hidden)
                        .listRowBackground(SilverSurface())
                }
                if matches.isEmpty {
                    ContentUnavailableView("No matching playlists", systemImage: "magnifyingglass",
                                           description: Text("Try another search or station. Older collections are available in Settings."))
                        .listRowBackground(HiFi.paper)
                } else {
                    ForEach(CatalogDay.grouped(matches)) { group in
                        Section {
                            ForEach(group.playlists) { playlist in
                                Button { open(playlist) } label: {
                                    HStack(spacing: 12) {
                                        PlaylistSummary(playlist: playlist, showStation: station.isEmpty)
                                        Image(systemName: "arrow.up.right")
                                            .font(.body.weight(.medium))
                                            .foregroundStyle(HiFi.blue)
                                            .accessibilityHidden(true)
                                    }
                                    .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                                    .padding(.vertical, 5)
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)
                                .listRowBackground(HiFi.paper)
                                .accessibilityLabel("\(playlist.station), \(playlist.title), \(playlist.trackCount) tracks")
                                .accessibilityValue(playlist.artistSample)
                                .accessibilityHint("Opens this playlist in Spotify")
                                .accessibilityIdentifier("playlist-\(playlist.id)")
                            }
                        } header: {
                            HStack(alignment: .firstTextBaseline) {
                                Text(group.label)
                                Spacer(minLength: 8)
                                Text("\(group.playlists.count)").monospacedDigit()
                            }
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(HiFi.ink)
                            .textCase(nil)
                        }
                    }
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .background(HiFi.paper)
            .scrollDismissesKeyboard(.interactively)
            .refreshable { await store.refresh() }
            .accessibilityIdentifier("catalog-list")
        }
    }

    private var statusBar: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Image(systemName: "internaldrive").accessibilityHidden(true)
            if let snapshot = store.snapshot {
                Text("\(matches.count) playlists · Updated \(snapshot.fetchedAt, style: .relative) ago")
            } else { Text("No saved catalog") }
            Spacer(minLength: 0)
        }
        .font(.caption).foregroundStyle(HiFi.secondary)
        .padding(.horizontal, 14).padding(.vertical, 10)
        .fixedSize(horizontal: false, vertical: true)
    }

    private func refreshError(_ error: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(error, systemImage: "exclamationmark.triangle")
                .font(.subheadline)
            HStack {
                Button { Task { await store.refresh() } } label: {
                    Text("Retry")
                        .frame(minWidth: 44, minHeight: 44)
                        .contentShape(Rectangle())
                }
                Spacer()
                Button { store.error = nil } label: {
                    Text("Dismiss")
                        .frame(minWidth: 44, minHeight: 44)
                        .contentShape(Rectangle())
                }
            }
        }
        .foregroundStyle(HiFi.ink)
        .padding(.horizontal, 14).padding(.top, 10)
        .background(Color(red: 1, green: 0.94, blue: 0.80))
    }

    private func open(_ playlist: Playlist) {
        UIApplication.shared.open(playlist.spotifyURL, options: [:]) { success in
            if !success {
                Task { @MainActor in unavailablePlaylist = playlist }
            }
        }
    }
}
