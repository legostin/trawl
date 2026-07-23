import { invoke } from "@tauri-apps/api/core";
import type { FieldInfo } from "@/lib/analyze";
import type * as monacoNs from "monaco-editor";
import { extractPathLiterals, pathArgContext } from "./pathContext";

let hintFields: FieldInfo[] = [];
let hintPattern = "";

/** Hint context: structure of past responses + the current rule's pattern. */
export function setPathHintContext(fields: FieldInfo[], pattern: string) {
  hintFields = fields;
  hintPattern = pattern;
}

/** Candidates for the next segment. prefix is up to the last './[' (partial word cut off). */
export function segmentCandidates(
  prefix: string,
  fields: FieldInfo[],
): { label: string; kind: "field" | "array"; type?: string }[] {
  // Convert a JSONPath prefix into the FieldInfo path form: "$.items[?…]." → "items[]"
  const norm = prefix
    .replace(/^\$\.?/, "")
    .replace(/\[[^\]]*\]/g, "[]")
    .replace(/\.+$/, "");
  const base = norm === "" ? "" : norm + ".";
  const seen = new Map<string, { kind: "field" | "array"; type?: string }>();
  for (const fi of fields) {
    if (base && !fi.path.startsWith(base)) continue;
    const rest = base ? fi.path.slice(base.length) : fi.path;
    const seg = rest.split(".")[0];
    if (!seg) continue;
    const isArr = seg.endsWith("[]");
    const name = isArr ? seg.slice(0, -2) : seg;
    if (!name) continue;
    const prev = seen.get(name);
    if (!prev || (isArr && prev.kind === "field")) {
      seen.set(name, { kind: isArr ? "array" : "field", type: rest === seg && !isArr ? fi.type : prev?.type });
    }
  }
  return [...seen.entries()].map(([label, v]) => ({ label, ...v }));
}

/** One-time registration of completion/inlay providers for javascript. */
export function registerPathHints(m: typeof monacoNs) {
  m.languages.registerCompletionItemProvider("javascript", {
    triggerCharacters: ["'", '"', ".", "["],
    provideCompletionItems(model, position) {
      const line = model.getLineContent(position.lineNumber);
      const ctx = pathArgContext(line, position.column);
      if (!ctx) return { suggestions: [] };
      const cut = ctx.prefix.replace(/[^.\[\]]*$/, "");
      const word = model.getWordUntilPosition(position);
      const range = new m.Range(position.lineNumber, word.startColumn, position.lineNumber, word.endColumn);
      return {
        suggestions: segmentCandidates(cut, hintFields).map((c) => ({
          label: c.type ? `${c.label}: ${c.type}` : c.label,
          filterText: c.label,
          sortText: c.label,
          kind: c.kind === "array" ? m.languages.CompletionItemKind.Struct : m.languages.CompletionItemKind.Field,
          insertText: c.kind === "array" ? `${c.label}[*]` : c.label,
          detail: c.kind === "array" ? "array" : c.type,
          range,
        })),
      };
    },
  });

  // Inlay: " → N nodes" after each path literal (based on the last matching flow).
  const countCache = new Map<string, { at: number; text: string | null }>();
  m.languages.registerInlayHintsProvider("javascript", {
    async provideInlayHints(model, range) {
      const hints: monacoNs.languages.InlayHint[] = [];
      const lits = extractPathLiterals(model.getValue()).filter(
        (l) => l.line >= range.startLineNumber && l.line <= range.endLineNumber,
      );
      for (const lit of lits) {
        const key = hintPattern + "\n" + lit.path;
        const cached = countCache.get(key);
        let text: string | null;
        if (cached && Date.now() - cached.at < 3000) {
          text = cached.text;
        } else {
          text = await invoke<{ nodes: number | null } | null>("test_path", { path: lit.path, pattern: hintPattern })
            .then((r) => (r == null || r.nodes == null ? null : r.nodes === 0 ? " → 0 nodes (no matches)" : ` → ${r.nodes} nodes`))
            .catch(() => null);
          countCache.set(key, { at: Date.now(), text });
        }
        if (text) {
          hints.push({
            position: { lineNumber: lit.line, column: lit.endColumn + 1 },
            label: text,
            paddingLeft: true,
          });
        }
      }
      return { hints, dispose() {} };
    },
  });
}

/** Debounced validation of JSONPath literals; markers under invalid paths. */
export function attachPathDiagnostics(editor: monacoNs.editor.IStandaloneCodeEditor) {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const validateNow = async () => {
    const model = editor.getModel();
    if (!model) return;
    // Import monaco-editor directly (not monaco-setup) — otherwise a circular import:
    // monaco-setup itself imports pathHints for registerPathHints.
    const monaco = await import("monaco-editor");
    const lits = extractPathLiterals(model.getValue());
    const markers: monacoNs.editor.IMarkerData[] = [];
    for (const lit of lits) {
      const err = await invoke<string | null>("validate_jsonpath", { path: lit.path }).catch(() => null);
      if (err) {
        markers.push({
          severity: monaco.MarkerSeverity.Error,
          message: `JSONPath: ${err}`,
          startLineNumber: lit.line,
          startColumn: lit.startColumn,
          endLineNumber: lit.line,
          endColumn: lit.endColumn,
        });
      }
    }
    monaco.editor.setModelMarkers(model, "trawl-jsonpath", markers);
  };
  editor.onDidChangeModelContent(() => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => void validateNow(), 400);
  });
  void validateNow();
}
