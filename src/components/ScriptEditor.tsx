import Editor, { type OnMount } from "@monaco-editor/react";
import type { MutableRefObject } from "react";
import "../monaco-setup";
import { useTheme } from "./ThemeProvider";

export interface ScriptEditorApi {
  /** Insert text at the current cursor/selection. */
  insert: (text: string) => void;
  /** Replace the whole document (kept in the undo stack). */
  replaceAll: (text: string) => void;
  /** Current selection text (empty when nothing is selected). */
  getSelectionText: () => string;
  /** Full document text. */
  getValue: () => string;
}

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

  const handleMount: OnMount = (editor) => {
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
