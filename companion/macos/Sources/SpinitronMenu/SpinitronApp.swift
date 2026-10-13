import AppKit
import SwiftUI
import SpinitronCore

@MainActor
extension CatalogStore {
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
            let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 420, height: 290),
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
