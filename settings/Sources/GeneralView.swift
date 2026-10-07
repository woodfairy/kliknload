import AppKit
import SwiftUI

struct GeneralView: View {
    @EnvironmentObject var store: Store

    var body: some View {
        Form {
            Section {
                HStack(spacing: 14) {
                    Image(nsImage: NSApp.applicationIconImage)
                        .resizable()
                        .frame(width: 56, height: 56)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: "kliknload").font(.title2.weight(.semibold))
                        Text("Click'n'Load receiver · version \(store.version)")
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 4)
            }

            Section {
                Picker("Language", selection: $store.config.language) {
                    Text(verbatim: "English").tag("en")
                    Text(verbatim: "Deutsch").tag("de")
                    Text("System language").tag("system")
                }
            } footer: {
                Text("Applies to the menu bar, notifications and this window.")
                    .foregroundStyle(.secondary)
            }

            Section {
                TextField("Listen address", text: $store.config.listen)
                    .font(.body.monospaced())
            } header: {
                Text(verbatim: "Click'n'Load")
            } footer: {
                Text("Browsers always send Click'n'Load to 127.0.0.1:9666. A change takes effect after restarting kliknload.")
                    .foregroundStyle(.secondary)
            }

            Section("Behavior") {
                Picker("Notifications", selection: $store.config.notifications) {
                    Text("All").tag("all")
                    Text("Errors only").tag("errors")
                    Text("Off").tag("off")
                }
                Toggle("Copy links to the clipboard if no output succeeds", isOn: $store.config.clipboardFallback)
                Toggle("Remove duplicate links", isOn: $store.config.dedupeLinks)
            }

            Section {
                LabeledContent("Status") {
                    Label(notificationLabel.0, systemImage: notificationLabel.1)
                        .foregroundStyle(notificationLabel.2)
                }
                HStack {
                    Button("Open System Settings") { store.openNotificationSettings() }
                    Button("Send test") { Task { await store.sendTestNotification() } }
                    if let result = store.notificationTestResult {
                        Text(result).foregroundStyle(.secondary)
                    }
                    Spacer()
                }
            } header: {
                Text("Notifications")
            } footer: {
                if store.notificationStatus == "denied" {
                    Text("macOS does not ask again. Allow notifications for kliknload in System Settings.")
                        .foregroundStyle(.secondary)
                }
            }
            .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
                Task { await store.refreshNotificationStatus() }
            }

            Section("System") {
                Toggle("Start at login", isOn: Binding(
                    get: { store.autostart },
                    set: { on in Task { await store.setAutostart(on) } }
                ))
            }

            Section {
                LabeledContent("File") {
                    Text(store.configPath)
                        .font(.callout.monospaced())
                        .textSelection(.enabled)
                        .lineLimit(2)
                        .truncationMode(.middle)
                }
                HStack {
                    Button("Show in Finder") { store.revealConfig() }
                    Button("Open log") { store.openLog() }
                    Spacer()
                    Button("Reload") { Task { await store.load() } }
                }
            } header: {
                Text("Configuration")
            } footer: {
                Text("All settings live in this JSON file and can be edited there too. kliknload picks up changes automatically. ${NAME} in values is replaced by the environment variable NAME.")
                    .foregroundStyle(.secondary)
            }

            if !store.problems.isEmpty {
                Section("Problems") {
                    ForEach(store.problems, id: \.self) { p in
                        Label(p, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(.orange)
                    }
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle("General")
    }

    private var notificationLabel: (String, String, Color) {
        switch store.notificationStatus {
        case "granted": (String(localized: "Allowed"), "checkmark.circle.fill", .green)
        case "denied": (String(localized: "Turned off in System Settings"), "bell.slash.fill", .orange)
        case "notDetermined": (String(localized: "Not asked yet – “Send test” asks"), "questionmark.circle", .secondary)
        default: (String(localized: "Unknown"), "questionmark.circle", .secondary)
        }
    }
}
