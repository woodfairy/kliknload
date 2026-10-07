import AppKit
import SwiftUI

/// Monospaced multi-line editor without smart quotes (they would break JSON and shell code).
struct CodeEditor: NSViewRepresentable {
    @Binding var text: String
    var minHeight: CGFloat = 100

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSTextView.scrollableTextView()
        let view = scroll.documentView as! NSTextView
        view.font = .monospacedSystemFont(ofSize: NSFont.systemFontSize, weight: .regular)
        view.isAutomaticQuoteSubstitutionEnabled = false
        view.isAutomaticDashSubstitutionEnabled = false
        view.isAutomaticTextReplacementEnabled = false
        view.isAutomaticSpellingCorrectionEnabled = false
        view.isContinuousSpellCheckingEnabled = false
        view.isRichText = false
        view.allowsUndo = true
        view.textContainerInset = NSSize(width: 4, height: 6)
        view.drawsBackground = true
        view.backgroundColor = .textBackgroundColor
        view.delegate = context.coordinator
        view.string = text
        scroll.borderType = .bezelBorder
        scroll.hasVerticalScroller = true
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.parent = self
        let view = scroll.documentView as! NSTextView
        if view.string != text {
            view.string = text
        }
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSScrollView, context: Context) -> CGSize? {
        CGSize(width: proposal.width ?? 400, height: minHeight)
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: CodeEditor
        init(_ parent: CodeEditor) { self.parent = parent }

        func textDidChange(_ notification: Notification) {
            guard let view = notification.object as? NSTextView else { return }
            parent.text = view.string
        }
    }
}

/// Single-line template input with the variables help next to it.
struct TemplateField: View {
    let title: String
    @Binding var text: String
    var footnote: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            LabeledContent(title) {
                HStack {
                    TextField("", text: $text).labelsHidden().font(.body.monospaced())
                    TemplateHelpButton()
                }
            }
            if let footnote {
                Text(footnote).font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

/// Popover listing template variables and filters. Clicking one copies it.
struct TemplateHelpButton: View {
    @EnvironmentObject var store: Store
    @State private var shown = false
    @State private var copied: String?

    var body: some View {
        Button {
            shown.toggle()
        } label: {
            Image(systemName: "curlybraces.square")
        }
        .buttonStyle(.borderless)
        .help("Variablen und Filter")
        .popover(isPresented: $shown, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: 10) {
                Text("Variablen").font(.headline)
                grid(store.meta.variables) { "{{\($0)}}" }
                Divider()
                Text("Filter").font(.headline)
                grid(store.meta.filters) { "|\($0)" }
                Text("Beispiel: {{links|json}}, {{package|safe}}. Klick kopiert.")
                    .font(.caption).foregroundStyle(.secondary)
                if let copied {
                    Text("Kopiert: \(copied)").font(.caption).foregroundStyle(.green)
                }
            }
            .padding(14)
            .frame(width: 420)
        }
    }

    private func grid(_ items: [NamedHelp], format: @escaping (String) -> String) -> some View {
        Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 4) {
            ForEach(items, id: \.name) { item in
                GridRow {
                    Button(format(item.name)) {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(format(item.name), forType: .string)
                        copied = format(item.name)
                    }
                    .buttonStyle(.link)
                    .font(.callout.monospaced())
                    Text(item.description).font(.callout).foregroundStyle(.secondary)
                }
            }
        }
    }
}

/// Editable list of name/value pairs (headers, form fields).
struct KeyValueEditor: View {
    @Binding var items: [KeyValue]
    let namePrompt: String
    let valuePrompt: String
    let addLabel: String

    var body: some View {
        ForEach($items) { $item in
            HStack(spacing: 8) {
                TextField("", text: $item.name, prompt: Text(namePrompt))
                    .labelsHidden()
                    .frame(maxWidth: 200)
                TextField("", text: $item.value, prompt: Text(valuePrompt))
                    .labelsHidden()
                    .font(.body.monospaced())
                Button {
                    items.removeAll { $0.id == item.id }
                } label: {
                    Image(systemName: "minus.circle.fill").foregroundStyle(.secondary)
                }
                .buttonStyle(.borderless)
            }
        }
        Button {
            items.append(KeyValue())
        } label: {
            Label(addLabel, systemImage: "plus.circle")
        }
        .buttonStyle(.borderless)
    }
}
