import { useState } from "react";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { Select } from "./ui/select";
import { parseUrlEncoded } from "@/lib/params";

export type BodyFormat = "raw" | "json" | "form" | "multipart";

type Row = { key: string; value: string };

const MULTIPART_BOUNDARY = "----trawlBoundary7MA4YWxkTrZu0gW";

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
export function BodyEditor({
  initialBody,
  initialContentType,
  onChange,
}: {
  initialBody: string;
  initialContentType: string;
  onChange: (r: { body: string; contentType: string }) => void;
}) {
  const [fmt, setFmt] = useState<BodyFormat>(detectBodyFormat(initialContentType));
  const [text, setText] = useState(initialBody);
  const [rows, setRows] = useState<Row[]>(() =>
    parseUrlEncoded(initialBody).map(([key, value]) => ({ key, value })),
  );

  const emit = (nextFmt: BodyFormat, nextText: string, nextRows: Row[]) => {
    const body =
      nextFmt === "form"
        ? serializeForm(nextRows)
        : nextFmt === "multipart"
          ? serializeMultipart(nextRows)
          : nextText;
    onChange({ body, contentType: contentTypeFor(nextFmt, initialContentType) });
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
    }
    setFmt(next);
    emit(next, nextText, nextRows);
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

  return (
    <div className="flex flex-col gap-2">
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
        <textarea
          value={text}
          onChange={(e) => changeText(e.target.value)}
          spellCheck={false}
          className="h-64 w-full resize-y rounded border border-border bg-card p-2 font-mono text-xs"
          placeholder={fmt === "json" ? "{ }" : ""}
        />
      )}
    </div>
  );
}
