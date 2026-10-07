import AppKit
import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }
}

@main
struct SettingsApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @StateObject private var store = Store()

    var body: some Scene {
        Window("kliknload Einstellungen", id: "settings") {
            ContentView()
                .environmentObject(store)
                .frame(minWidth: 820, minHeight: 560)
                .task { await store.load() }
        }
        .windowResizability(.contentMinSize)
        .commands {
            CommandGroup(replacing: .newItem) {}
            CommandGroup(after: .saveItem) {
                Button("Neu laden") { Task { await store.load() } }
                    .keyboardShortcut("r")
            }
        }
    }
}

enum Selection: Hashable {
    case general
    case output(String)
}

struct ContentView: View {
    @EnvironmentObject var store: Store
    @State private var selection: Selection? = .general

    var body: some View {
        NavigationSplitView {
            Sidebar(selection: $selection)
                .navigationSplitViewColumnWidth(min: 210, ideal: 230, max: 300)
        } detail: {
            Group {
                switch selection {
                case .output(let id):
                    if let index = store.config.outputs.firstIndex(where: { $0.id == id }) {
                        OutputView(output: $store.config.outputs[index], onDelete: {
                            store.removeOutput(id: id)
                            selection = .general
                        })
                        .id(id)
                    } else {
                        GeneralView()
                    }
                default:
                    GeneralView()
                }
            }
            .safeAreaInset(edge: .top, spacing: 0) { Banner() }
        }
        .toolbar {
            ToolbarItem(placement: .status) {
                if store.saving {
                    ProgressView().controlSize(.small)
                } else if store.saveError == nil && store.loadError == nil {
                    Label("Gespeichert", systemImage: "checkmark.circle")
                        .labelStyle(.iconOnly)
                        .foregroundStyle(.secondary)
                        .help("Änderungen werden automatisch gespeichert")
                }
            }
        }
    }
}

/// Load and save errors across the top of the detail pane.
struct Banner: View {
    @EnvironmentObject var store: Store

    var body: some View {
        if let message = store.loadError ?? store.saveError {
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.yellow)
                Text(message).font(.callout).textSelection(.enabled)
                Spacer()
                Button("Neu laden") { Task { await store.load() } }
            }
            .padding(10)
            .background(.bar)
            .overlay(alignment: .bottom) { Divider() }
        }
    }
}

struct Sidebar: View {
    @EnvironmentObject var store: Store
    @Binding var selection: Selection?

    var body: some View {
        List(selection: $selection) {
            Section {
                Label("Allgemein", systemImage: "gearshape").tag(Selection.general)
            }
            Section("Ausgaben") {
                ForEach($store.config.outputs) { $output in
                    OutputRow(output: $output).tag(Selection.output(output.id))
                }
                .onMove { store.config.outputs.move(fromOffsets: $0, toOffset: $1) }
            }
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom) {
            HStack(spacing: 0) {
                AddOutputMenu { id in selection = .output(id) }
                Button {
                    if case .output(let id) = selection {
                        store.removeOutput(id: id)
                        selection = .general
                    }
                } label: {
                    Image(systemName: "minus").frame(width: 24, height: 20)
                }
                .buttonStyle(.borderless)
                .disabled({ if case .output = selection { return false } else { return true } }())
                .help("Ausgewählte Ausgabe entfernen")
                Spacer()
            }
            .padding(8)
        }
    }
}

struct OutputRow: View {
    @Binding var output: OutputConfig

    var body: some View {
        HStack {
            Label(output.displayName, systemImage: output.kind.symbol)
                .foregroundStyle(output.enabled ? .primary : .secondary)
            Spacer()
            Toggle("", isOn: $output.enabled)
                .toggleStyle(.switch)
                .controlSize(.mini)
                .labelsHidden()
                .help(output.enabled ? "Aktiv" : "Inaktiv")
        }
    }
}

struct AddOutputMenu: View {
    @EnvironmentObject var store: Store
    var added: (String) -> Void

    var body: some View {
        Menu {
            ForEach(OutputType.allCases) { type in
                if type == .http {
                    Menu {
                        ForEach(store.meta.httpPresets) { preset in
                            Button(preset.label) {
                                var o = preset.output
                                o.name = preset.label
                                added(store.addOutput(o))
                            }
                        }
                    } label: {
                        Label(type.label, systemImage: type.symbol)
                    }
                } else {
                    Button {
                        added(store.addOutput(store.defaults(for: type)))
                    } label: {
                        Label(type.label, systemImage: type.symbol)
                    }
                }
            }
        } label: {
            Image(systemName: "plus").frame(width: 24, height: 20)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .help("Ausgabe hinzufügen")
    }
}
