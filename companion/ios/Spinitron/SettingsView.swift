import SwiftUI
import SpinitronCore
import SpinitronUI

struct SettingsView: View {
    @ObservedObject var store: CatalogStore
    @AppStorage("defaultStation") private var defaultStation = "KALX"
    @AppStorage("includeLegacy") private var includeLegacy = false
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section("Browsing") {
                    Picker("Default station", selection: $defaultStation) {
                        Text("All stations").tag("")
                        ForEach(store.stations, id: \.self) { Text($0).tag($0) }
                    }
                    .accessibilityIdentifier("default-station")
                    Toggle("Include older playlist collections", isOn: $includeLegacy)
                }
                Section {
                    if let snapshot = store.snapshot {
                        LabeledContent("Saved playlists", value: "\(snapshot.playlists.count)")
                        LabeledContent("Last refreshed") {
                            Text(snapshot.fetchedAt, format: .dateTime.month().day().hour().minute())
                        }
                    }
                    Button { Task { await store.refresh() } } label: {
                        HStack {
                            Text(store.isRefreshing ? "Refreshing…" : "Refresh catalog")
                            Spacer()
                            if store.isRefreshing { ProgressView() }
                        }
                    }.disabled(store.isRefreshing)
                    if let error = store.error {
                        Text(error).foregroundStyle(.red).font(.subheadline)
                    }
                } header: { Text("Offline catalog") } footer: {
                    Text("The last downloaded catalog is available offline. Refresh or pull down on the list to fetch new playlists. Playing music still requires Spotify. Broadcast dates use Pacific time.")
                }
                Section {
                    LabeledContent("Appearance", value: "Silver Hi-Fi")
                    Text("Spinitron reads the public broadcast catalog and opens playlists in Spotify. It does not sign in to Spotify or change your library.")
                        .font(.subheadline).foregroundStyle(.secondary)
                }
            }
            .tint(HiFi.blue)
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }.accessibilityIdentifier("done-settings")
                }
            }
        }
    }
}
