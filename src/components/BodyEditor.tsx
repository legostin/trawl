import { useState } from "react";
import Editor from "@monaco-editor/react";
import "../monaco-setup";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Select } from "./ui/select";
import { useTheme } from "./ThemeProvider";
import { parseUrlEncoded } from "@/lib/params";

export type BodyFormat = "raw" | "json" | "form" | "multipart";

type Row = { key: string; value: string };

const MULTIPART_BOUNDARY = "----trawlBoundary7MA4YWxkTrZu0gW";

/** Monaco language for syntax highlighting, from format + content-type. */
function monacoLanguage(fmt: BodyFormat, contentType: string): string {
  if (fmt === "json") return "json";
  const ct = contentType.toLowerCase();
  if (ct.includes("json")) return "json";
  if (ct.includes("html")) return "html";
  if (ct.includes("xml")) return "xml";
  if (ct.includes("javascript")) return "javascript";
  if (ct.includes("css")) return "css";
  return "plaintext";
}

/** Pretty-print JSON; returns the input unchanged if it isn't valid JSON. */
function prettifyJson(s: string): string {
  const t = s.trim();
  if (!t) return s;
  try {
    return JSON.stringify(JSON.parse(t), null, 2);
  } catch {
    return s;
  }
}

/** Pick the initial body format from a content-type header. */
export function detectBodyFormat(contentType: string): BodyFormat {
  const ct = contentType.toLowerCase();
  if (ct.includes("application/json")) return "json";
  if (ct.includes("multipart/form-data")) return "multipart";
  if (ct.includes("x-www-form-urlencoded")) return "form";
  return "raw";
}

/** Content-type a format implies. `raw` keeps whatever was already set. */
function contentTypeFor(fmt: BodyFormat, current: string): string {
  switch (fmt) {
    case "json":
      return "application/json";
    case "form":
      return "application/x-www-form-urlencoded";
    case "multipart":
      return `multipart/form-data; boundary=${MULTIPART_BOUNDARY}`;
    case "raw":
      return current;
  }
}

function serializeForm(rows: Row[]): string {
  return rows
    .filter((r) => r.key.trim() !== "")
    .map((r) => `${encodeURIComponent(r.key)}=${encodeURIComponent(r.value)}`)
    .join("&");
}

function serializeMultipart(rows: Row[]): string {
  const parts = rows
    .filter((r) => r.key.trim() !== "")
    .map(
      (r) =>
        `--${MULTIPART_BOUNDARY}\r\nContent-Disposition: form-data; name="${r.key}"\r\n\r\n${r.value}\r\n`,
    );
  return parts.length > 0 ? parts.join("") + `--${MULTIPART_BOUNDARY}--\r\n` : "";
}

/**
 * Body editor with a format selector (raw / json / form / multipart). The format
 * defaults from the content-type but can be switched; switching serializes the
 * body accordingly and reports the matching content-type back to the parent.
 */
type FileBody = { name: string; base64: string; size: number; type: string };

export function BodyEditor({
  initialBody,
  initialContentType,
  allowFile,
  onChange,
}: {
  initialBody: string;
  initialContentType: string;
  /** Show a "Replace with file…" uploader (raw bytes replace the body). */
  allowFile?: boolean;
  onChange: (r: { body: string; contentType: string; bodyBase64?: string }) => void;
}) {
  const initialFmt = detectBodyFormat(initialContentType);
  const { theme } = useTheme();
  const [fmt, setFmt] = useState<BodyFormat>(initialFmt);
  const [text, setText] = useState(initialFmt === "json" ? prettifyJson(initialBody) : initialBody);
  const [rows, setRows] = useState<Row[]>(() =>
    parseUrlEncoded(initialBody).map(([key, value]) => ({ key, value })),
  );
  const [file, setFile] = useState<FileBody | null>(null);

  const emit = (nextFmt: BodyFormat, nextText: string, nextRows: Row[]) => {
    const body =
      nextFmt === "form"
        ? serializeForm(nextRows)
        : nextFmt === "multipart"
          ? serializeMultipart(nextRows)
          : nextText;
    onChange({ body, contentType: contentTypeFor(nextFmt, initialContentType) });
  };

  const onPickFile = (f: File) => {
    const reader = new FileReader();
    reader.onload = () => {
      const base64 = String(reader.result).split(",")[1] ?? "";
      const picked = { name: f.name, base64, size: f.size, type: f.type };
      setFile(picked);
      onChange({
        body: "",
        contentType: picked.type || contentTypeFor(fmt, initialContentType),
        bodyBase64: picked.base64,
      });
    };
    reader.readAsDataURL(f);
  };

  const clearFile = () => {
    setFile(null);
    emit(fmt, text, rows);
  };

  const changeFormat = (next: BodyFormat) => {
    // Moving into a key/value format: parse the current text as pairs.
    // Moving out: flatten the pairs back into the text field.
    let nextRows = rows;
    let nextText = text;
    const wasKv = fmt === "form" || fmt === "multipart";
    const isKv = next === "form" || next === "multipart";
    if (isKv && !wasKv) {
      nextRows = parseUrlEncoded(text).map(([key, value]) => ({ key, value }));
      setRows(nextRows);
    } else if (!isKv && wasKv) {
      nextText = serializeForm(rows);
      setText(nextText);
    } else if (next === "json") {
      nextText = prettifyJson(text);
      setText(nextText);
    }
    setFmt(next);
    emit(next, nextText, nextRows);
  };

  const prettify = () => {
    const next = prettifyJson(text);
    setText(next);
    emit(fmt, next, rows);
  };

  const patchRow = (i: number, p: Partial<Row>) => {
    const next = rows.map((r, j) => (j === i ? { ...r, ...p } : r));
    setRows(next);
    emit(fmt, text, next);
  };
  const removeRow = (i: number) => {
    const next = rows.filter((_, j) => j !== i);
    setRows(next);
    emit(fmt, text, next);
  };
  const addRow = () => {
    const next = [...rows, { key: "", value: "" }];
    setRows(next);
    emit(fmt, text, next);
  };

  const changeText = (v: string) => {
    setText(v);
    emit(fmt, v, rows);
  };

  const isKv = fmt === "form" || fmt === "multipart";

  if (file) {
    return (
      <div className="flex items-center gap-2 rounded border border-border bg-card p-3 text-xs">
        <span className="font-mono">📎 {file.name}</span>
        <span className="text-muted-foreground">
          {file.type || "application/octet-stream"} · {file.size} bytes
        </span>
        <Button size="sm" variant="ghost" className="ml-auto h-6 text-[11px]" onClick={clearFile}>
          Clear
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <label className="flex items-center gap-1 text-xs text-muted-foreground">
          Format
          <Select
            value={fmt}
            onChange={(e) => changeFormat(e.target.value as BodyFormat)}
            className="h-7"
          >
            <option value="raw">Raw</option>
            <option value="json">JSON</option>
            <option value="form">Form (urlencoded)</option>
            <option value="multipart">Multipart</option>
          </Select>
        </label>
        {fmt === "json" && (
          <Button size="sm" variant="ghost" className="h-6 text-[11px]" onClick={prettify}>
            Prettify
          </Button>
        )}
        {allowFile && (
          <label className="ml-auto cursor-pointer text-[11px] text-primary hover:underline">
            Replace with file…
            <input
              type="file"
              className="hidden"
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (f) onPickFile(f);
                e.target.value = "";
              }}
            />
          </label>
        )}
      </div>

      {isKv ? (
        <div>
          <table className="mb-2 w-full border-collapse text-xs">
            <tbody>
              {rows.map((r, i) => (
                <tr key={i} className="border-b border-border/50">
                  <td className="w-1/3 py-1 pr-2">
                    <Input
                      value={r.key}
                      onChange={(e) => patchRow(i, { key: e.target.value })}
                      className="h-6 font-mono"
                      placeholder="name"
                    />
                  </td>
                  <td className="py-1 pr-2">
                    <Input
                      value={r.value}
                      onChange={(e) => patchRow(i, { value: e.target.value })}
                      className="h-6 font-mono"
                      placeholder="value"
                    />
                  </td>
                  <td className="w-8 py-1">
                    <Button size="iconSm" variant="ghost" title="Remove" onClick={() => removeRow(i)}>
                      ×
                    </Button>
                  </td>
                </tr>
              ))}
              {rows.length === 0 && (
                <tr>
                  <td colSpan={3} className="py-2 text-muted-foreground">
                    None
                  </td>
                </tr>
              )}
            </tbody>
          </table>
          <Button size="sm" variant="ghost" onClick={addRow}>
            + Add field
          </Button>
        </div>
      ) : (
        <div className="h-72 overflow-hidden rounded border border-border">
          <Editor
            height="100%"
            language={monacoLanguage(fmt, initialContentType)}
            theme={theme === "dark" ? "vs-dark" : "light"}
            value={text}
            onChange={(v) => changeText(v ?? "")}
            options={{
              minimap: { enabled: false },
              fontSize: 12,
              scrollBeyondLastLine: false,
              automaticLayout: true,
              tabSize: 2,
              lineNumbersMinChars: 3,
              wordWrap: "on",
              padding: { top: 6 },
            }}
          />
        </div>
      )}
    </div>
  );
}
