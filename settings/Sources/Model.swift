import Foundation

// Mirrors the JSON config of the Rust core (src/config.rs). Unknown keys are kept by the
// core when saving, so this only needs the fields the window edits.

struct KeyValue: Codable, Hashable, Identifiable {
    var id = UUID()
    var name: String = ""
    var value: String = ""

    enum CodingKeys: String, CodingKey { case name, value }
}

struct HttpAuth: Codable, Hashable {
    var type: String = "none"
    var username: String = ""
    var password: String = ""
    var token: String = ""

    enum CodingKeys: String, CodingKey { case type, username, password, token }

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        type = try c.decodeIfPresent(String.self, forKey: .type) ?? "none"
        username = try c.decodeIfPresent(String.self, forKey: .username) ?? ""
        password = try c.decodeIfPresent(String.self, forKey: .password) ?? ""
        token = try c.decodeIfPresent(String.self, forKey: .token) ?? ""
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(type, forKey: .type)
        switch type {
        case "basic":
            try c.encode(username, forKey: .username)
            try c.encode(password, forKey: .password)
        case "bearer":
            try c.encode(token, forKey: .token)
        default:
            break
        }
    }
}

enum OutputType: String, CaseIterable, Identifiable {
    case pyload, clipboard, file, http, command
    var id: String { rawValue }

    var label: String {
        switch self {
        case .pyload: "pyLoad"
        case .clipboard: "Zwischenablage"
        case .file: "Datei"
        case .http: "HTTP-Request"
        case .command: "Befehl"
        }
    }

    var symbol: String {
        switch self {
        case .pyload: "arrow.down.circle"
        case .clipboard: "doc.on.clipboard"
        case .file: "doc.text"
        case .http: "network"
        case .command: "terminal"
        }
    }

    var summary: String {
        switch self {
        case .pyload: "Pakete inklusive Passwort an eine pyLoad-Instanz senden"
        case .clipboard: "Links in die Zwischenablage kopieren"
        case .file: "Links als Textdatei, JSON, CSV oder JDownloader-Crawljob speichern"
        case .http: "Beliebigen HTTP-Request bauen, z.B. Webhook, aria2, ntfy, Discord"
        case .command: "Ein Shell-Kommando ausführen, Daten kommen per Umgebungsvariablen"
        }
    }
}

/// One output. A flat struct holding the fields of every type; only the ones that belong
/// to `type` are written back.
struct OutputConfig: Codable, Identifiable, Hashable {
    var id: String = ""
    var name: String = ""
    var enabled: Bool = true
    var type: String = "clipboard"
    var include: String = ""
    var exclude: String = ""

    // pyload
    var url: String = ""
    var user: String = ""
    var password: String = ""
    var apiKey: String = ""
    var destination: String = "queue"
    var packageName: String = "{{package}}"

    // clipboard + file (custom format)
    var template: String = ""

    // file
    var directory: String = ""
    var filename: String = ""
    var format: String = "txt"
    var mode: String = "new"
    var onConflict: String = "number"

    // http
    var method: String = "POST"
    var headers: [KeyValue] = []
    var auth = HttpAuth()
    var bodyType: String = "json"
    var body: String = ""
    var form: [KeyValue] = []
    var perLink: Bool = false
    var timeoutSecs: Int = 30
    var insecureTls: Bool = false

    // command
    var command: String = ""
    var workingDir: String = ""

    var kind: OutputType { OutputType(rawValue: type) ?? .clipboard }
    var displayName: String { name.trimmingCharacters(in: .whitespaces).isEmpty ? kind.label : name }

    enum CodingKeys: String, CodingKey {
        case id, name, enabled, type, include, exclude
        case url, user, password, apiKey, destination, packageName
        case template, directory, filename, format, mode, onConflict
        case method, headers, auth, bodyType, body, form, perLink, timeoutSecs, insecureTls
        case command, workingDir
    }

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        func s(_ k: CodingKeys, _ d: String = "") throws -> String { try c.decodeIfPresent(String.self, forKey: k) ?? d }
        id = try s(.id)
        name = try s(.name)
        enabled = try c.decodeIfPresent(Bool.self, forKey: .enabled) ?? true
        type = try s(.type, "clipboard")
        include = try s(.include)
        exclude = try s(.exclude)
        url = try s(.url)
        user = try s(.user)
        password = try s(.password)
        apiKey = try s(.apiKey)
        destination = try s(.destination, "queue")
        packageName = try s(.packageName, "{{package}}")
        template = try s(.template)
        directory = try s(.directory)
        filename = try s(.filename)
        format = try s(.format, "txt")
        mode = try s(.mode, "new")
        onConflict = try s(.onConflict, "number")
        method = try s(.method, "POST")
        headers = try c.decodeIfPresent([KeyValue].self, forKey: .headers) ?? []
        auth = try c.decodeIfPresent(HttpAuth.self, forKey: .auth) ?? HttpAuth()
        bodyType = try s(.bodyType, "none")
        body = try s(.body)
        form = try c.decodeIfPresent([KeyValue].self, forKey: .form) ?? []
        perLink = try c.decodeIfPresent(Bool.self, forKey: .perLink) ?? false
        timeoutSecs = try c.decodeIfPresent(Int.self, forKey: .timeoutSecs) ?? 30
        insecureTls = try c.decodeIfPresent(Bool.self, forKey: .insecureTls) ?? false
        command = try s(.command)
        workingDir = try s(.workingDir)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(name, forKey: .name)
        try c.encode(enabled, forKey: .enabled)
        try c.encode(type, forKey: .type)
        if !include.isEmpty { try c.encode(include, forKey: .include) }
        if !exclude.isEmpty { try c.encode(exclude, forKey: .exclude) }
        switch kind {
        case .pyload:
            try c.encode(url, forKey: .url)
            try c.encode(user, forKey: .user)
            try c.encode(password, forKey: .password)
            try c.encode(apiKey, forKey: .apiKey)
            try c.encode(destination, forKey: .destination)
            try c.encode(packageName, forKey: .packageName)
        case .clipboard:
            try c.encode(template, forKey: .template)
        case .file:
            try c.encode(directory, forKey: .directory)
            try c.encode(filename, forKey: .filename)
            try c.encode(format, forKey: .format)
            try c.encode(template, forKey: .template)
            try c.encode(mode, forKey: .mode)
            try c.encode(onConflict, forKey: .onConflict)
        case .http:
            try c.encode(method, forKey: .method)
            try c.encode(url, forKey: .url)
            try c.encode(headers, forKey: .headers)
            try c.encode(auth, forKey: .auth)
            try c.encode(bodyType, forKey: .bodyType)
            try c.encode(body, forKey: .body)
            try c.encode(form, forKey: .form)
            try c.encode(perLink, forKey: .perLink)
            try c.encode(timeoutSecs, forKey: .timeoutSecs)
            try c.encode(insecureTls, forKey: .insecureTls)
        case .command:
            try c.encode(command, forKey: .command)
            try c.encode(workingDir, forKey: .workingDir)
            try c.encode(perLink, forKey: .perLink)
            try c.encode(timeoutSecs, forKey: .timeoutSecs)
        }
    }
}

struct AppConfig: Codable, Equatable {
    var listen: String = "127.0.0.1:9666"
    var notifications: String = "all"
    var clipboardFallback: Bool = true
    var dedupeLinks: Bool = true
    var outputs: [OutputConfig] = []

    enum CodingKeys: String, CodingKey { case listen, notifications, clipboardFallback, dedupeLinks, outputs }

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        listen = try c.decodeIfPresent(String.self, forKey: .listen) ?? "127.0.0.1:9666"
        notifications = try c.decodeIfPresent(String.self, forKey: .notifications) ?? "all"
        clipboardFallback = try c.decodeIfPresent(Bool.self, forKey: .clipboardFallback) ?? true
        dedupeLinks = try c.decodeIfPresent(Bool.self, forKey: .dedupeLinks) ?? true
        outputs = try c.decodeIfPresent([OutputConfig].self, forKey: .outputs) ?? []
    }
}

struct NamedHelp: Codable, Hashable {
    var name: String
    var description: String
}

struct HttpPreset: Codable, Hashable, Identifiable {
    var id: String
    var label: String
    var output: OutputConfig
}

struct Meta: Codable {
    var variables: [NamedHelp] = []
    var filters: [NamedHelp] = []
    var httpPresets: [HttpPreset] = []
    var outputDefaults: [OutputConfig] = []
}

struct ConfigResponse: Codable {
    var configPath: String
    var logPath: String
    var version: String
    var autostart: Bool
    var loadError: String?
    var problems: [String]
    var config: AppConfig
    var meta: Meta
}

struct SaveResponse: Codable {
    var ok: Bool
    var problems: [String]?
}

struct RunResponse: Codable {
    var ok: Bool
    var text: String?
    var error: String?
}
