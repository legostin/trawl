import Editor from "@monaco-editor/react";
import "../monaco-setup";
import { useTheme } from "./ThemeProvider";

export function ScriptEditor({
  value,
  onChange,
  language = "javascript",
}: {
  value: string;
  onChange: (v: string) => void;
  language?: string;
}) {
  const { theme } = useTheme();
  return (
    <Editor
      height="100%"
      language={language}
      theme={theme === "dark" ? "vs-dark" : "light"}
      value={value}
      onChange={(v) => onChange(v ?? "")}
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
