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
                        Text("kliknload").font(.title2.weight(.semibold))
                        Text("Click'n'Load-Empfänger · Version \(store.version)")
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.vertical, 4)
            }

            Section {
                TextField("Listen-Adresse", text: $store.config.listen)
                    .font(.body.monospaced())
            } header: {
                Text("Click'n'Load")
            } footer: {
                Text("Browser senden Click'n'Load immer an 127.0.0.1:9666. Eine Änderung wird nach einem Neustart von kliknload wirksam.")
                    .foregroundStyle(.secondary)
            }

            Section("Verhalten") {
                Picker("Mitteilungen", selection: $store.config.notifications) {
                    Text("Alle").tag("all")
                    Text("Nur Fehler").tag("errors")
                    Text("Aus").tag("off")
                }
                Toggle(isOn: $store.config.clipboardFallback) {
                    Text("Links in die Zwischenablage, wenn keine Ausgabe klappt")
                }
                Toggle("Doppelte Links entfernen", isOn: $store.config.dedupeLinks)
            }

            Section("System") {
                Toggle("Bei der Anmeldung starten", isOn: Binding(
                    get: { store.autostart },
                    set: { on in Task { await store.setAutostart(on) } }
                ))
            }

            Section {
                LabeledContent("Datei") {
                    Text(store.configPath)
                        .font(.callout.monospaced())
                        .textSelection(.enabled)
                        .lineLimit(2)
                        .truncationMode(.middle)
                }
                HStack {
                    Button("Im Finder zeigen") { store.revealConfig() }
                    Button("Log öffnen") { store.openLog() }
                    Spacer()
                    Button("Neu laden") { Task { await store.load() } }
                }
            } header: {
                Text("Konfiguration")
            } footer: {
                Text("Alle Einstellungen stehen in dieser JSON-Datei und können auch direkt dort bearbeitet werden. kliknload übernimmt Änderungen automatisch. ${NAME} in Werten wird durch die Umgebungsvariable NAME ersetzt.")
                    .foregroundStyle(.secondary)
            }

            if !store.problems.isEmpty {
                Section("Hinweise") {
                    ForEach(store.problems, id: \.self) { p in
                        Label(p, systemImage: "exclamationmark.triangle")
                            .foregroundStyle(.orange)
                    }
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle("Allgemein")
    }
}
