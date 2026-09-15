import AppKit
import SwiftUI
import SpinitronCore

@MainActor
final class CatalogStore: ObservableObject {
    @Published var snapshot: CatalogSnapshot?
    @Published var isRefreshing = false
    @Published var error: String?
    private var started = false
    private let repository: CatalogRepository

    init() {
        let cache = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("net.fiftyfootfoghorn.spinitron/catalog.json")
        repository = CatalogRepository(cacheURL: cache)
    }

    var stations: [String] { Array(Set((snapshot?.playlists.map(\.station) ?? []) + ["KALX"])).sorted() }

    func start() async {
        guard !started else { return }
        started = true
        snapshot = try? await repository.cached()
        if snapshot == nil { await refresh() }
    }

    func refresh() async {
        guard !isRefreshing else { return }
        isRefreshing = true
        error = nil
        defer { isRefreshing = false }
        do { snapshot = try await repository.refresh() }
        catch { self.error = "Couldn’t refresh: \(error.localizedDescription)" }
    }

    func open(_ playlist: Playlist) {
        guard let app = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.spotify.client") else {
            error = "Install the Spotify app to open playlists."
            return
        }
        let configuration = NSWorkspace.OpenConfiguration()
        NSWorkspace.shared.open([playlist.spotifyURL], withApplicationAt: app, configuration: configuration) { _, failure in
            if let failure {
                Task { @MainActor in self.error = "Couldn’t open Spotify: \(failure.localizedDescription)" }
            }
        }
    }
}

@main
enum SpinitronApp {
    @MainActor static func main() {
        let app = NSApplication.shared
        let delegate = AppDelegate()
        app.delegate = delegate
        app.setActivationPolicy(.accessory)
        withExtendedLifetime(delegate) { app.run() }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let store = CatalogStore()
    private var statusItem: NSStatusItem!
    private let popover = NSPopover()
    private var settingsWindow: NSWindow?

    func applicationDidFinishLaunching(_ notification: Notification) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        if let button = statusItem.button {
            button.image = NSImage(systemSymbolName: "radio", accessibilityDescription: "Spinitron")
            button.toolTip = "Spinitron — browse radio playlists"
            button.target = self
            button.action = #selector(togglePopover)
        }
        popover.behavior = .transient
        popover.contentSize = NSSize(width: 520, height: 640)
        popover.contentViewController = NSHostingController(rootView:
            CatalogView(store: store, showSettings: { [weak self] in self?.openSettings() }))
        showPopover()
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        showPopover()
        return false
    }

    @objc private func togglePopover() {
        if popover.isShown { popover.performClose(nil) } else { showPopover() }
    }

    private func showPopover() {
        guard let button = statusItem?.button else { return }
        NSApp.activate(ignoringOtherApps: true)
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        popover.contentViewController?.view.window?.makeKey()
    }

    private func openSettings() {
        popover.performClose(nil)
        if settingsWindow == nil {
            let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 420, height: 215),
                                  styleMask: [.titled, .closable], backing: .buffered, defer: false)
            window.title = "Spinitron Settings"
            window.isReleasedWhenClosed = false
            window.contentViewController = NSHostingController(rootView: SettingsView(store: store))
            window.center()
            settingsWindow = window
        }
        NSApp.activate(ignoringOtherApps: true)
        settingsWindow?.makeKeyAndOrderFront(nil)
    }
}

private struct DayGroup: Identifiable {
    let id: Date
    let playlists: [Playlist]
}

struct CatalogView: View {
    @ObservedObject var store: CatalogStore
    let showSettings: () -> Void
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
        VStack(spacing: 0) {
            HStack {
                Text("Spinitron").font(.headline)
                Spacer()
                Button { Task { await store.refresh() } } label: {
                    Label("Refresh", systemImage: "arrow.clockwise")
                }
                .disabled(store.isRefreshing)
                .keyboardShortcut("r")
                .help("Download the latest catalog (⌘R)")
                Button(action: showSettings) { Image(systemName: "gearshape") }
                    .help("Settings")
                    .accessibilityLabel("Settings")
            }
            .padding(14)

            HStack(spacing: 10) {
                HStack(spacing: 6) {
                    Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                    TextField("Search shows, artists or songs", text: $query)
                        .textFieldStyle(.plain)
                        .focused($searching)
                        .accessibilityIdentifier("catalog-search")
                    if !query.isEmpty {
                        Button { query = "" } label: { Image(systemName: "xmark.circle.fill") }
                            .buttonStyle(.plain).foregroundStyle(.secondary).help("Clear search")
                    }
                }
                .padding(8)
                .background(.quaternary, in: RoundedRectangle(cornerRadius: 6))
                Picker("Station", selection: $station) {
                    Text("All stations").tag("")
                    ForEach(store.stations, id: \.self) { Text($0).tag($0) }
                }
                .labelsHidden()
                .frame(width: 125)
            }
            .padding(.horizontal, 14)
            .padding(.bottom, 12)

            if let error = store.error {
                HStack(alignment: .top) {
                    Image(systemName: "exclamationmark.triangle").foregroundStyle(.orange)
                    Text(error).font(.caption).textSelection(.enabled)
                    Spacer(minLength: 0)
                    Button { store.error = nil } label: { Image(systemName: "xmark") }
                        .buttonStyle(.plain).accessibilityLabel("Dismiss error")
                }
                .padding(12)
                .background(.orange.opacity(0.08))
            }
            Divider()
            if store.snapshot == nil {
                VStack(spacing: 12) {
                    if store.isRefreshing {
                        ProgressView()
                        Text("Loading playlists…")
                    } else {
                        Image(systemName: "antenna.radiowaves.left.and.right").font(.largeTitle)
                        Text("Refresh to load the catalog")
                        Button("Refresh") { Task { await store.refresh() } }
                    }
                }
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if matches.isEmpty {
                ContentUnavailableView("No matching playlists", systemImage: "magnifyingglass",
                    description: Text("Try another search or station. Older collections are available in Settings."))
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0, pinnedViews: .sectionHeaders) {
                        ForEach(groups) { group in
                            Section {
                                ForEach(group.playlists) { playlist in
                                    PlaylistRow(playlist: playlist, showStation: station.isEmpty) { store.open(playlist) }
                                    Divider().padding(.leading, 14)
                                }
                            } header: {
                                HStack {
                                    Text(dayLabel(group.id))
                                    Spacer()
                                    Text("\(group.playlists.count)")
                                }
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(.secondary)
                                .padding(.horizontal, 14).padding(.vertical, 8)
                                .background(.regularMaterial)
                            }
                        }
                    }
                }
            }
            Divider()
            HStack(spacing: 8) {
                if store.isRefreshing { ProgressView().controlSize(.small) }
                if let snapshot = store.snapshot {
                    Text("\(matches.count) playlists · Refreshed \(snapshot.fetchedAt, style: .relative) ago")
                        .help(snapshot.fetchedAt.formatted(date: .abbreviated, time: .shortened))
                } else { Text("No saved catalog") }
                Spacer()
                Button("Quit") { NSApplication.shared.terminate(nil) }.keyboardShortcut("q")
            }
            .font(.caption).foregroundStyle(.secondary).padding(12)
        }
        .frame(width: 520, height: 640)
        .task { await store.start() }
        .onChange(of: defaultStation) { _, value in station = value }
        .background {
            Button("Find") { searching = true }.keyboardShortcut("f").hidden()
        }
    }

    private func dayLabel(_ date: Date) -> String {
        if date == .distantPast { return "Date unavailable" }
        let formatter = DateFormatter()
        formatter.timeZone = TimeZone(identifier: "America/Los_Angeles")
        formatter.dateFormat = "EEEE, MMM d, yyyy"
        return formatter.string(from: date)
    }
}

struct PlaylistRow: View {
    let playlist: Playlist
    let showStation: Bool
    let open: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: open) {
            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(playlist.title).font(.system(size: 13, weight: .medium)).lineLimit(2)
                    if !playlist.artistSample.isEmpty {
                        Text(playlist.artistSample).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                    HStack(spacing: 6) {
                        if showStation { Text(playlist.station) }
                        if !playlist.isBroadcast { Text("Older collection") }
                        Text("\(playlist.trackCount) tracks")
                    }.font(.system(size: 10)).foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
                Image(systemName: "arrow.up.forward.app")
                    .foregroundStyle(hovered ? Color.accentColor : Color.secondary)
                    .padding(.top, 2)
            }
            .padding(.horizontal, 14).padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
            .background(hovered ? Color.accentColor.opacity(0.09) : .clear)
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
        Form {
            Picker("Default station", selection: $defaultStation) {
                Text("All stations").tag("")
                ForEach(store.stations, id: \.self) { Text($0).tag($0) }
            }
            Toggle("Include older playlist collections", isOn: $includeLegacy)
            Text("The catalog is saved on this Mac. Use Refresh to download new playlists. Broadcast dates use Pacific time.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .formStyle(.grouped)
        .frame(width: 420, height: 215)
        .task { await store.start() }
    }
}
