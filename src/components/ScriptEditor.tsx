import Editor, { type OnMount } from "@monaco-editor/react";
import { useRef, type MutableRefObject } from "react";
import type { editor as MonacoEditor } from "monaco-editor";
import "../monaco-setup";
import { attachPathDiagnostics } from "../scripting/pathHints";
import { useTheme } from "./ThemeProvider";

// One definition, shared with plugins: the host hands this very component to
// them, so a second copy of the type is a copy that drifts.
export type { ScriptEditorApi } from "@/plugins/api";
import type { ScriptEditorApi } from "@/plugins/api";

export function ScriptEditor({
  value,
  onChange,
  language = "javascript",
  apiRef,
}: {
  value: string;
  onChange: (v: string) => void;
  language?: string;
  apiRef?: MutableRefObject<ScriptEditorApi | null>;
}) {
  const { theme } = useTheme();
  const decorations = useRef<MonacoEditor.IEditorDecorationsCollection | null>(null);

  const handleMount: OnMount = (editor) => {
    if (language === "javascript") attachPathDiagnostics(editor);
    if (!apiRef) return;
    apiRef.current = {
      insert: (text) => {
        const sel = editor.getSelection();
        if (sel) editor.executeEdits("snippet", [{ range: sel, text, forceMoveMarkers: true }]);
        editor.focus();
      },
      replaceAll: (text) => {
        const model = editor.getModel();
        if (model) {
          editor.executeEdits("template", [
            { range: model.getFullModelRange(), text, forceMoveMarkers: true },
          ]);
        }
        editor.focus();
      },
      getSelectionText: () => {
        const sel = editor.getSelection();
        const model = editor.getModel();
        return sel && model ? model.getValueInRange(sel) : "";
      },
      getValue: () => editor.getModel()?.getValue() ?? "",

      /** Mark lines a caller cares about — a failing step, say — and show them. */
      highlightLines: (lines, kind = "error") => {
        decorations.current?.clear();
        if (!lines.length) return;
        decorations.current = editor.createDecorationsCollection(
          lines.map((line) => ({
            range: { startLineNumber: line, startColumn: 1, endLineNumber: line, endColumn: 1 },
            options: {
              isWholeLine: true,
              className: `trawl-line-${kind}`,
              linesDecorationsClassName: `trawl-gutter-${kind}`,
            },
          })),
        );
        editor.revealLineInCenterIfOutsideViewport(lines[0]!);
      },

      insertLines: (at, text) => {
        const model = editor.getModel();
        if (!model) return;
        const line = Math.max(1, Math.min(at, model.getLineCount() + 1));
        editor.executeEdits("insert-lines", [
          {
            range: { startLineNumber: line, startColumn: 1, endLineNumber: line, endColumn: 1 },
            text: text.endsWith("\n") ? text : `${text}\n`,
            forceMoveMarkers: true,
          },
        ]);
      },
    };
  };

  return (
    <Editor
      height="100%"
      language={language}
      theme={theme === "dark" ? "vs-dark" : "light"}
      value={value}
      onChange={(v) => onChange(v ?? "")}
      onMount={handleMount}
      options={{
        minimap: { enabled: false },
        fontSize: 13,
        scrollBeyondLastLine: false,
        automaticLayout: true,
        tabSize: 2,
        lineNumbersMinChars: 3,
        padding: { top: 8 },
      }}
    />
  );
}
