import Combine
import Foundation

/// Shared cache-first lifecycle. Opening another app belongs to each platform.
@MainActor
public final class CatalogStore: ObservableObject {
    @Published public private(set) var snapshot: CatalogSnapshot?
    @Published public private(set) var isRefreshing = false
    @Published public var error: String?
    private var started = false
    private let repository: CatalogRepository

    public init(repository: CatalogRepository) {
        self.repository = repository
    }

    public convenience init() {
        let cache = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("net.fiftyfootfoghorn.spinitron/catalog.json")
        self.init(repository: CatalogRepository(cacheURL: cache))
    }

    public var stations: [String] {
        Array(Set((snapshot?.playlists.map(\.station) ?? []) + ["KALX"])).sorted()
    }

    public func start() async {
        guard !started else { return }
        started = true
        snapshot = try? await repository.cached()
        if snapshot == nil { await refresh() }
    }

    public func refresh() async {
        guard !isRefreshing else { return }
        isRefreshing = true
        error = nil
        defer { isRefreshing = false }
        do { snapshot = try await repository.refresh() }
        catch let error as URLError {
            let reason: String
            switch error.code {
            case .notConnectedToInternet: reason = "You’re offline."
            case .timedOut: reason = "The connection timed out."
            default: reason = "The catalog server couldn’t be reached."
            }
            self.error = "Couldn’t refresh: \(reason) " + (snapshot == nil
                ? "Check your connection and try again."
                : "Your saved catalog is still available.")
        }
        catch { self.error = "Couldn’t refresh: \(error.localizedDescription)" }
    }
}
