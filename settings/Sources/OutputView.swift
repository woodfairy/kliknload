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
                        Text("Enabled")
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
                TextField("Only links matching", text: $output.include, prompt: Text(verbatim: "rapidgator|ddownload"))
                    .font(.body.monospaced())
                TextField("Exclude links matching", text: $output.exclude, prompt: Text(verbatim: "\\.sfv$"))
                    .font(.body.monospaced())
            } header: {
                Text("Link filter")
            } footer: {
                Text("Regular expressions. Leave empty to pass on all links.").foregroundStyle(.secondary)
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
                    Button("Remove output…", role: .destructive) { confirmDelete = true }
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle(output.displayName)
        .navigationSubtitle(output.kind.label)
        .confirmationDialog("Remove “\(output.displayName)”?", isPresented: $confirmDelete) {
            Button("Remove", role: .destructive, action: onDelete)
        }
    }
}

// MARK: - pyLoad

struct PyloadSection: View {
    @Binding var output: OutputConfig

    var body: some View {
        Section {
            TextField("URL", text: $output.url, prompt: Text(verbatim: "https://pyload.example.org"))
            TextField("User", text: $output.user)
            SecureField("Password", text: $output.password)
            SecureField("API key", text: $output.apiKey, prompt: Text(verbatim: "pl_…"))
        } header: {
            Text(verbatim: "pyLoad")
        } footer: {
            Text("If an API key is set, it is used instead of user and password.").foregroundStyle(.secondary)
        }
        Section("Package") {
            Picker("Destination", selection: $output.destination) {
                Text("Queue").tag("queue")
                Text("Link collector").tag("collector")
            }
            TemplateField(title: "Package name", text: $output.packageName)
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
                Text("Content")
                Spacer()
                TemplateHelpButton()
            }
        } footer: {
            Text("Default: {{links}}, one link per line.").foregroundStyle(.secondary)
        }
    }
}

// MARK: - File

struct FileSection: View {
    @Binding var output: OutputConfig

    var body: some View {
        Section("Location") {
            LabeledContent("Folder") {
                HStack {
                    TextField("", text: $output.directory, prompt: Text(verbatim: "~/Downloads/kliknload"))
                        .labelsHidden()
                    Button("Choose…") { chooseFolder() }
                }
            }
            TemplateField(title: "File name", text: $output.filename,
                          footnote: "“/” creates subfolders, e.g. {{host}}/{{date}} {{package}}.txt")
        }
        Section {
            Picker("Format", selection: $output.format) {
                Text("Text, one link per line").tag("txt")
                Text(verbatim: "JSON").tag("json")
                Text(verbatim: "CSV").tag("csv")
                Text("JDownloader crawljob").tag("crawljob")
                Text("Own template").tag("custom")
            }
            if output.format == "custom" {
                CodeEditor(text: $output.template, minHeight: 100)
            }
            Picker("Write", selection: $output.mode) {
                Text("New file per package").tag("new")
                Text("Append to one file").tag("append")
            }
            Picker("If the file exists", selection: $output.onConflict) {
                Text("Number").tag("number")
                Text("Overwrite").tag("overwrite")
                Text("Skip").tag("skip")
            }
            .disabled(output.mode == "append")
        } header: {
            HStack {
                Text("Content")
                Spacer()
                TemplateHelpButton()
            }
        } footer: {
            Text(formatHelp).foregroundStyle(.secondary)
        }
    }

    private var formatHelp: String {
        switch output.format {
        case "json": String(localized: "One JSON object per package. JSON Lines when appending.")
        case "csv": String(localized: "Columns package, link, password, source, received. Header only at the start of the file.")
        case "crawljob": String(localized: "For JDownloader's folder watch directory. The password is included.")
        case "custom": String(localized: "The template is written once per package.")
        default: String(localized: "Only the links, one per line.")
        }
    }

    private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.prompt = String(localized: "Choose")
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
                    ForEach(methods, id: \.self) { Text(verbatim: $0).tag($0) }
                }
                .labelsHidden()
                .fixedSize()
                TextField("", text: $output.url, prompt: Text(verbatim: "https://example.org/hook?name={{package}}"))
                    .labelsHidden()
                    .font(.body.monospaced())
            }
        } header: {
            HStack {
                Text("Request")
                Spacer()
                Menu("Load preset") {
                    ForEach(store.meta.httpPresets) { preset in
                        Button(preset.label) { apply(preset.output) }
                    }
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
                TemplateHelpButton()
            }
        } footer: {
            Text("Variables in the URL are URL-encoded automatically.").foregroundStyle(.secondary)
        }

        Section("Headers") {
            KeyValueEditor(items: $output.headers, namePrompt: "Name", valuePrompt: "Value", addLabel: "Add header")
        }

        Section("Authentication") {
            Picker("Type", selection: $output.auth.type) {
                Text("None").tag("none")
                Text(verbatim: "Basic").tag("basic")
                Text("Bearer token").tag("bearer")
            }
            if output.auth.type == "basic" {
                TextField("User", text: $output.auth.username)
                SecureField("Password", text: $output.auth.password)
            } else if output.auth.type == "bearer" {
                SecureField("Token", text: $output.auth.token)
            }
        }

        Section {
            Picker("Body", selection: $output.bodyType) {
                Text("None").tag("none")
                Text(verbatim: "JSON").tag("json")
                Text("Form").tag("form")
                Text("Text").tag("text")
            }
            .pickerStyle(.segmented)
            switch output.bodyType {
            case "json", "text":
                CodeEditor(text: $output.body, minHeight: 140)
            case "form":
                KeyValueEditor(items: $output.form, namePrompt: "Field", valuePrompt: "Value", addLabel: "Add field")
            default:
                EmptyView()
            }
        } header: {
            Text("Body")
        } footer: {
            if output.bodyType == "json" {
                Text("Variables are inserted as JSON string content, e.g. \"{{package}}\". {{links|json}} turns the list into an array.")
                    .foregroundStyle(.secondary)
            }
        }

        Section("Options") {
            Toggle("One request per link ({{link}}, {{index}})", isOn: $output.perLink)
            Stepper("Timeout: \(output.timeoutSecs) s", value: $output.timeoutSecs, in: 1...600)
            Toggle("Accept invalid TLS certificates", isOn: $output.insecureTls)
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
            LabeledContent("Working folder") {
                TextField("", text: $output.workingDir, prompt: Text("optional"))
                    .labelsHidden()
            }
            Toggle("One run per link", isOn: $output.perLink)
            Stepper("Timeout: \(output.timeoutSecs) s", value: $output.timeoutSecs, in: 1...3600)
        } header: {
            Text("Command (/bin/sh)")
        } footer: {
            Text("Package data comes in environment variables, never directly in the command: $KLIKNLOAD_PACKAGE, $KLIKNLOAD_LINKS, $KLIKNLOAD_LINK, $KLIKNLOAD_INDEX, $KLIKNLOAD_COUNT, $KLIKNLOAD_PASSWORD, $KLIKNLOAD_SOURCE, $KLIKNLOAD_HOST, $KLIKNLOAD_DATE, $KLIKNLOAD_JSON. The links are also on stdin, one per line. Always put variables in quotes.")
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
                    Label(testResult.ok ? (testResult.text ?? "OK") : (testResult.error ?? String(localized: "Error")),
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
            Text("Preview with sample data")
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
        case .pyload: String(localized: "Test connection")
        case .http: String(localized: "Send test request")
        case .file: String(localized: "Write test file")
        case .command: String(localized: "Run test")
        case .clipboard: String(localized: "Copy sample")
        }
    }

    private var testFootnote: String {
        switch output.kind {
        case .pyload: String(localized: "The test only logs in, no package is added.")
        default: String(localized: "The test really runs the output, with the sample package above.")
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
