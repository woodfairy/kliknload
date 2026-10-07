import AppKit
import SwiftUI

struct OutputView: View {
    @EnvironmentObject var store: Store
    @Binding var output: OutputConfig
    var onDelete: () -> Void
    @State private var confirmDelete = false

    var body: some View {
        Form {
            Section {
                Toggle(isOn: $output.enabled) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Aktiv")
                        Text(output.kind.summary).font(.callout).foregroundStyle(.secondary)
                    }
                }
                TextField("Name", text: $output.name)
            }

            switch output.kind {
            case .pyload: PyloadSection(output: $output)
            case .clipboard: ClipboardSection(output: $output)
            case .file: FileSection(output: $output)
            case .http: HttpSections(output: $output)
            case .command: CommandSection(output: $output)
            }

            Section {
                TextField("Nur Links, die passen", text: $output.include, prompt: Text("z.B. rapidgator|ddownload"))
                    .font(.body.monospaced())
                TextField("Links ausschließen", text: $output.exclude, prompt: Text("z.B. \\.sfv$"))
                    .font(.body.monospaced())
            } header: {
                Text("Link-Filter")
            } footer: {
                Text("Reguläre Ausdrücke. Leer lassen, um alle Links weiterzugeben.").foregroundStyle(.secondary)
            }

            let problems = store.problems(for: output)
            if !problems.isEmpty {
                Section {
                    ForEach(problems, id: \.self) { p in
                        Label(p, systemImage: "exclamationmark.triangle").foregroundStyle(.orange)
                    }
                }
            }

            PreviewSection(output: output)

            Section {
                HStack {
                    Spacer()
                    Button("Ausgabe entfernen…", role: .destructive) { confirmDelete = true }
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle(output.displayName)
        .navigationSubtitle(output.kind.label)
        .confirmationDialog("„\(output.displayName)“ entfernen?", isPresented: $confirmDelete) {
            Button("Entfernen", role: .destructive, action: onDelete)
        }
    }
}

// MARK: - pyLoad

struct PyloadSection: View {
    @Binding var output: OutputConfig

    var body: some View {
        Section {
            TextField("URL", text: $output.url, prompt: Text("https://pyload.example.org"))
            TextField("Benutzer", text: $output.user)
            SecureField("Passwort", text: $output.password)
            SecureField("API-Key", text: $output.apiKey, prompt: Text("optional, pl_…"))
        } header: {
            Text("pyLoad")
        } footer: {
            Text("Ist ein API-Key gesetzt, wird er statt Benutzer und Passwort verwendet.").foregroundStyle(.secondary)
        }
        Section("Paket") {
            Picker("Ziel", selection: $output.destination) {
                Text("Warteschlange").tag("queue")
                Text("Linksammler").tag("collector")
            }
            TemplateField(title: "Paketname", text: $output.packageName)
        }
    }
}

// MARK: - Clipboard

struct ClipboardSection: View {
    @Binding var output: OutputConfig

    var body: some View {
        Section {
            CodeEditor(text: $output.template, minHeight: 70)
        } header: {
            HStack {
                Text("Inhalt")
                Spacer()
                TemplateHelpButton()
            }
        } footer: {
            Text("Standard: {{links}}, ein Link pro Zeile.").foregroundStyle(.secondary)
        }
    }
}

// MARK: - File

struct FileSection: View {
    @Binding var output: OutputConfig

    var body: some View {
        Section("Speicherort") {
            LabeledContent("Ordner") {
                HStack {
                    TextField("", text: $output.directory, prompt: Text("~/Downloads/kliknload"))
                        .labelsHidden()
                    Button("Auswählen…") { chooseFolder() }
                }
            }
            TemplateField(title: "Dateiname", text: $output.filename,
                          footnote: "„/“ legt Unterordner an, z.B. {{host}}/{{date}} {{package}}.txt")
        }
        Section {
            Picker("Format", selection: $output.format) {
                Text("Text, ein Link pro Zeile").tag("txt")
                Text("JSON").tag("json")
                Text("CSV").tag("csv")
                Text("JDownloader-Crawljob").tag("crawljob")
                Text("Eigene Vorlage").tag("custom")
            }
            if output.format == "custom" {
                CodeEditor(text: $output.template, minHeight: 100)
            }
            Picker("Schreiben", selection: $output.mode) {
                Text("Neue Datei pro Paket").tag("new")
                Text("An eine Datei anhängen").tag("append")
            }
            Picker("Wenn die Datei existiert", selection: $output.onConflict) {
                Text("Nummerieren").tag("number")
                Text("Überschreiben").tag("overwrite")
                Text("Überspringen").tag("skip")
            }
            .disabled(output.mode == "append")
        } header: {
            HStack {
                Text("Inhalt")
                Spacer()
                TemplateHelpButton()
            }
        } footer: {
            Text(formatHelp).foregroundStyle(.secondary)
        }
    }

    private var formatHelp: String {
        switch output.format {
        case "json": "Ein JSON-Objekt pro Paket. Beim Anhängen als JSON Lines."
        case "csv": "Spalten package, link, password, source, received. Kopfzeile nur am Dateianfang."
        case "crawljob": "Für den Folder-Watch-Ordner von JDownloader. Passwort wird mit übergeben."
        case "custom": "Die Vorlage wird einmal pro Paket geschrieben."
        default: "Nur die Links, je einer pro Zeile."
        }
    }

    private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = "Auswählen"
        if panel.runModal() == .OK, let url = panel.url {
            let home = FileManager.default.homeDirectoryForCurrentUser.path
            output.directory = url.path.hasPrefix(home) ? "~" + url.path.dropFirst(home.count) : url.path
        }
    }
}

// MARK: - HTTP

struct HttpSections: View {
    @EnvironmentObject var store: Store
    @Binding var output: OutputConfig

    private let methods = ["GET", "POST", "PUT", "PATCH", "DELETE"]

    var body: some View {
        Section {
            HStack(spacing: 8) {
                Picker("", selection: $output.method) {
                    ForEach(methods, id: \.self) { Text($0).tag($0) }
                }
                .labelsHidden()
                .fixedSize()
                TextField("", text: $output.url, prompt: Text("https://example.org/hook?name={{package}}"))
                    .labelsHidden()
                    .font(.body.monospaced())
            }
        } header: {
            HStack {
                Text("Anfrage")
                Spacer()
                Menu("Vorlage laden") {
                    ForEach(store.meta.httpPresets) { preset in
                        Button(preset.label) { apply(preset.output) }
                    }
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
                TemplateHelpButton()
            }
        } footer: {
            Text("Variablen in der URL werden automatisch URL-kodiert.").foregroundStyle(.secondary)
        }

        Section("Header") {
            KeyValueEditor(items: $output.headers, namePrompt: "Name", valuePrompt: "Wert", addLabel: "Header hinzufügen")
        }

        Section("Authentifizierung") {
            Picker("Art", selection: $output.auth.type) {
                Text("Keine").tag("none")
                Text("Basic").tag("basic")
                Text("Bearer-Token").tag("bearer")
            }
            if output.auth.type == "basic" {
                TextField("Benutzer", text: $output.auth.username)
                SecureField("Passwort", text: $output.auth.password)
            } else if output.auth.type == "bearer" {
                SecureField("Token", text: $output.auth.token)
            }
        }

        Section {
            Picker("Body", selection: $output.bodyType) {
                Text("Kein").tag("none")
                Text("JSON").tag("json")
                Text("Formular").tag("form")
                Text("Text").tag("text")
            }
            .pickerStyle(.segmented)
            switch output.bodyType {
            case "json", "text":
                CodeEditor(text: $output.body, minHeight: 140)
            case "form":
                KeyValueEditor(items: $output.form, namePrompt: "Feld", valuePrompt: "Wert", addLabel: "Feld hinzufügen")
            default:
                EmptyView()
            }
        } header: {
            Text("Body")
        } footer: {
            if output.bodyType == "json" {
                Text("Variablen werden als JSON-String-Inhalt eingesetzt, z.B. \"{{package}}\". Mit {{links|json}} wird die Liste zum Array.")
                    .foregroundStyle(.secondary)
            }
        }

        Section("Optionen") {
            Toggle("Ein Request pro Link ({{link}}, {{index}})", isOn: $output.perLink)
            Stepper("Timeout: \(output.timeoutSecs) s", value: $output.timeoutSecs, in: 1...600)
            Toggle("Ungültige TLS-Zertifikate akzeptieren", isOn: $output.insecureTls)
        }
    }

    private func apply(_ preset: OutputConfig) {
        output.method = preset.method
        output.url = preset.url
        output.headers = preset.headers
        output.auth = preset.auth
        output.bodyType = preset.bodyType
        output.body = preset.body
        output.form = preset.form
        output.perLink = preset.perLink
    }
}

// MARK: - Command

struct CommandSection: View {
    @Binding var output: OutputConfig

    var body: some View {
        Section {
            CodeEditor(text: $output.command, minHeight: 80)
            LabeledContent("Arbeitsordner") {
                TextField("", text: $output.workingDir, prompt: Text("optional"))
                    .labelsHidden()
            }
            Toggle("Ein Aufruf pro Link", isOn: $output.perLink)
            Stepper("Timeout: \(output.timeoutSecs) s", value: $output.timeoutSecs, in: 1...3600)
        } header: {
            Text("Befehl (/bin/sh)")
        } footer: {
            Text("""
            Paketdaten kommen aus Umgebungsvariablen, nie direkt in den Befehl: \
            $KLIKNLOAD_PACKAGE, $KLIKNLOAD_LINKS, $KLIKNLOAD_LINK, $KLIKNLOAD_INDEX, $KLIKNLOAD_COUNT, \
            $KLIKNLOAD_PASSWORD, $KLIKNLOAD_SOURCE, $KLIKNLOAD_HOST, $KLIKNLOAD_DATE, $KLIKNLOAD_JSON. \
            Die Links stehen außerdem zeilenweise auf stdin. Variablen immer in Anführungszeichen setzen.
            """)
            .foregroundStyle(.secondary)
        }
    }
}

// MARK: - Preview & test

struct PreviewSection: View {
    @EnvironmentObject var store: Store
    let output: OutputConfig
    @State private var preview: RunResponse?
    @State private var testResult: RunResponse?
    @State private var testing = false

    var body: some View {
        Section {
            if let preview {
                ScrollView(.horizontal) {
                    Text(preview.ok ? (preview.text ?? "") : (preview.error ?? ""))
                        .font(.callout.monospaced())
                        .foregroundStyle(preview.ok ? Color.primary : Color.red)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.vertical, 4)
                }
            } else {
                ProgressView().controlSize(.small)
            }
            HStack {
                if let testResult {
                    Label(testResult.ok ? (testResult.text ?? "OK") : (testResult.error ?? "Fehler"),
                          systemImage: testResult.ok ? "checkmark.circle.fill" : "xmark.octagon.fill")
                        .foregroundStyle(testResult.ok ? .green : .red)
                        .textSelection(.enabled)
                        .lineLimit(4)
                }
                Spacer()
                if testing { ProgressView().controlSize(.small) }
                Button(testLabel) { runTest() }
                    .disabled(testing)
            }
        } header: {
            Text("Vorschau mit Beispieldaten")
        } footer: {
            Text(testFootnote).foregroundStyle(.secondary)
        }
        .task(id: output) {
            // Debounce typing, then ask the core how this output would behave.
            try? await Task.sleep(nanoseconds: 300_000_000)
            guard !Task.isCancelled else { return }
            preview = await store.preview(output)
        }
    }

    private var testLabel: String {
        switch output.kind {
        case .pyload: "Verbindung testen"
        case .http: "Testanfrage senden"
        case .file: "Testdatei schreiben"
        case .command: "Testweise ausführen"
        case .clipboard: "Beispiel kopieren"
        }
    }

    private var testFootnote: String {
        switch output.kind {
        case .pyload: "Der Test meldet sich nur an, es wird kein Paket angelegt."
        default: "Der Test führt die Ausgabe wirklich aus, mit dem Beispielpaket oben."
        }
    }

    private func runTest() {
        testing = true
        testResult = nil
        Task {
            testResult = await store.test(output)
            testing = false
        }
    }
}
