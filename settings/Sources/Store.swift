import AppKit
import Foundation

/// Talks to the Rust core (`kliknload config get|set`, `kliknload output preview|test`).
/// The window has no logic of its own; validation, previews and tests all run in the core.
@MainActor
final class Store: ObservableObject {
    @Published var config = AppConfig() {
        didSet { if loaded && config != oldValue { scheduleSave() } }
    }
    @Published var problems: [String] = []
    @Published var loadError: String?
    @Published var saveError: String?
    @Published var autostart = false
    @Published var meta = Meta()
    @Published var configPath = ""
    @Published var logPath = ""
    @Published var version = ""
    @Published var saving = false
    /// granted | denied | notDetermined | unknown
    @Published var notificationStatus = "unknown"
    @Published var notificationTestResult: String?

    private var loaded = false
    private var saveTask: Task<Void, Never>?
    private let cli: URL
    private let configArg: [String]

    init() {
        let args = CommandLine.arguments
        func value(after flag: String) -> String? {
            guard let i = args.firstIndex(of: flag), i + 1 < args.count else { return nil }
            return args[i + 1]
        }
        // kliknload.app/Contents/Helpers/kliknload Settings.app -> kliknload.app/Contents/MacOS/kliknload
        let bundled = Bundle.main.bundleURL
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("MacOS/kliknload")
        cli = value(after: "--kliknload").map { URL(fileURLWithPath: $0) } ?? bundled
        configArg = value(after: "--config").map { ["--config", $0] } ?? []
    }

    // MARK: - Core calls

    nonisolated private static func run(_ cli: URL, _ args: [String], input: Data?) async throws -> Data {
        try await withCheckedThrowingContinuation { cont in
            DispatchQueue.global(qos: .userInitiated).async {
                let p = Process()
                p.executableURL = cli
                p.arguments = args
                let out = Pipe(), err = Pipe(), inp = Pipe()
                p.standardOutput = out
                p.standardError = err
                p.standardInput = inp
                do {
                    try p.run()
                    if let input { inp.fileHandleForWriting.write(input) }
                    try? inp.fileHandleForWriting.close()
                    let data = out.fileHandleForReading.readDataToEndOfFile()
                    let errData = err.fileHandleForReading.readDataToEndOfFile()
                    p.waitUntilExit()
                    if p.terminationStatus != 0 && data.isEmpty {
                        let msg = String(data: errData, encoding: .utf8) ?? "exit \(p.terminationStatus)"
                        cont.resume(throwing: NSError(domain: "kliknload", code: Int(p.terminationStatus),
                                                      userInfo: [NSLocalizedDescriptionKey: msg.trimmingCharacters(in: .whitespacesAndNewlines)]))
                    } else {
                        cont.resume(returning: data)
                    }
                } catch {
                    cont.resume(throwing: error)
                }
            }
        }
    }

    private func call<T: Decodable>(_ args: [String], input: Data? = nil) async throws -> T {
        let data = try await Store.run(cli, configArg + args, input: input)
        return try JSONDecoder().decode(T.self, from: data)
    }

    func load() async {
        do {
            let r: ConfigResponse = try await call(["config", "get"])
            loaded = false
            config = r.config
            problems = r.problems
            loadError = r.loadError
            autostart = r.autostart
            meta = r.meta
            configPath = r.configPath
            logPath = r.logPath
            version = r.version
            loaded = true
            await refreshNotificationStatus()
        } catch {
            loadError = "kliknload-Kern nicht erreichbar (\(cli.path)): \(error.localizedDescription)"
        }
    }

    private func scheduleSave() {
        saveTask?.cancel()
        saveTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 500_000_000)
            guard !Task.isCancelled else { return }
            await self?.save()
        }
    }

    func save() async {
        saving = true
        defer { saving = false }
        do {
            let data = try JSONEncoder().encode(config)
            let r: SaveResponse = try await call(["config", "set"], input: data)
            problems = r.problems ?? []
            saveError = nil
        } catch {
            saveError = "Speichern fehlgeschlagen: \(error.localizedDescription)"
        }
    }

    func preview(_ output: OutputConfig) async -> RunResponse {
        await runOutput("preview", output)
    }

    func test(_ output: OutputConfig) async -> RunResponse {
        await runOutput("test", output)
    }

    private func runOutput(_ action: String, _ output: OutputConfig) async -> RunResponse {
        do {
            let data = try JSONEncoder().encode(output)
            return try await call(["output", action], input: data)
        } catch {
            return RunResponse(ok: false, text: nil, error: error.localizedDescription)
        }
    }

    func setAutostart(_ on: Bool) async {
        struct R: Codable { var autostart: Bool }
        if let r: R = try? await call(["autostart", on ? "on" : "off"]) {
            autostart = r.autostart
        }
    }

    func refreshNotificationStatus() async {
        struct R: Codable { var status: String }
        if let r: R = try? await call(["notify-status"]) {
            notificationStatus = r.status
        }
    }

    func openNotificationSettings() {
        Task { _ = try? await Store.run(cli, ["notify-settings"], input: nil) }
    }

    func sendTestNotification() async {
        notificationTestResult = nil
        do {
            let data = try await Store.run(cli, configArg + ["notify-test"], input: nil)
            notificationTestResult = String(data: data, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines) == "notification sent"
                ? "Gesendet" : "Fehlgeschlagen"
        } catch {
            notificationTestResult = error.localizedDescription
        }
        await refreshNotificationStatus()
    }

    // MARK: - Editing helpers

    func problems(for output: OutputConfig) -> [String] {
        let prefix = "\(output.displayName): "
        return problems.filter { $0.hasPrefix(prefix) }.map { String($0.dropFirst(prefix.count)) }
    }

    func defaults(for type: OutputType) -> OutputConfig {
        meta.outputDefaults.first { $0.type == type.rawValue } ?? {
            var o = OutputConfig()
            o.type = type.rawValue
            o.name = type.label
            return o
        }()
    }

    /// Adds an output and returns its id.
    func addOutput(_ template: OutputConfig) -> String {
        var o = template
        o.enabled = true
        let base = o.type
        var id = base, n = 2
        while config.outputs.contains(where: { $0.id == id }) {
            id = "\(base)-\(n)"
            n += 1
        }
        o.id = id
        if config.outputs.contains(where: { $0.displayName == o.displayName }) {
            o.name = "\(o.displayName) \(n - 1)"
        }
        config.outputs.append(o)
        return id
    }

    func removeOutput(id: String) {
        config.outputs.removeAll { $0.id == id }
    }

    func revealConfig() {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: configPath)])
    }

    func openLog() {
        NSWorkspace.shared.open(URL(fileURLWithPath: logPath))
    }
}
