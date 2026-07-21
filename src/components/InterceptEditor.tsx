import { useState } from "react";
import { Ban, Check, Reply } from "lucide-react";
import { useFlows } from "../store";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { bodyToText } from "@/lib/body";
import type { Flow, Header } from "@/types";

type Row = { key: string; value: string };

function toRows(headers: Header[]): Row[] {
  return headers.map(([key, value]) => ({ key, value }));
}
function toHeaders(rows: Row[]): [string, string][] {
  return rows.filter((r) => r.key.trim() !== "").map((r) => [r.key, r.value]);
}

export function InterceptEditor({ flow }: { flow: Flow }) {
  const phase = flow.pausedPhase ?? "request";
  const resolve = useFlows((s) => s.resolveBreakpoint);

  const isRequest = phase === "request";
  const [method, setMethod] = useState(flow.method);
  const [status, setStatus] = useState(String(flow.response?.status ?? 200));
  const [rows, setRows] = useState<Row[]>(
    toRows(isRequest ? flow.request.headers : flow.response?.headers ?? []),
  );
  const [body, setBody] = useState(bodyToText(isRequest ? flow.request : flow.response));
  const [busy, setBusy] = useState(false);

  const patchRow = (i: number, p: Partial<Row>) =>
    setRows((rs) => rs.map((r, j) => (j === i ? { ...r, ...p } : r)));
  const removeRow = (i: number) => setRows((rs) => rs.filter((_, j) => j !== i));
  const addRow = () => setRows((rs) => [...rs, { key: "", value: "" }]);

  const act = async (action: "execute" | "abort" | "respond") => {
    setBusy(true);
    try {
      if (action === "abort") {
        await resolve(flow.id, phase, "abort", { reason: "aborted from UI" });
      } else if (action === "respond") {
        await resolve(flow.id, phase, "respond", {
          status: Number(status) || 200,
          headers: toHeaders(rows),
          body,
        });
      } else {
        await resolve(flow.id, phase, "execute", {
          method: isRequest ? method : undefined,
          status: isRequest ? undefined : Number(status) || 200,
          headers: toHeaders(rows),
          body,
        });
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-border bg-http-red/10 px-3 py-2">
        <span className="rounded bg-http-red px-1.5 py-0.5 text-[10px] font-semibold uppercase text-white">
          Paused · {phase}
        </span>
        <div className="ml-auto flex items-center gap-1">
          <Button size="sm" disabled={busy} onClick={() => void act("execute")}>
            <Check />
            Execute
          </Button>
          {isRequest && (
            <Button size="sm" variant="outline" disabled={busy} onClick={() => void act("respond")}>
              <Reply />
              Respond locally
            </Button>
          )}
          <Button size="sm" variant="destructive" disabled={busy} onClick={() => void act("abort")}>
            <Ban />
            Abort
          </Button>
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-3">
        <div className="mb-3 flex items-center gap-2">
          {isRequest ? (
            <label className="flex items-center gap-1 text-xs text-muted-foreground">
              Method
              <Input
                value={method}
                onChange={(e) => setMethod(e.target.value)}
                className="h-7 w-28 font-mono"
              />
            </label>
          ) : (
            <label className="flex items-center gap-1 text-xs text-muted-foreground">
              Status
              <Input
                value={status}
                onChange={(e) => setStatus(e.target.value)}
                className="h-7 w-24 font-mono"
              />
            </label>
          )}
        </div>

        <div className="mb-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
          Headers
        </div>
        <table className="mb-3 w-full border-collapse text-xs">
          <tbody>
            {rows.map((r, i) => (
              <tr key={i} className="border-b border-border/50">
                <td className="w-1/3 py-1 pr-2">
                  <Input
                    value={r.key}
                    onChange={(e) => patchRow(i, { key: e.target.value })}
                    className="h-6 font-mono"
                  />
                </td>
                <td className="py-1 pr-2">
                  <Input
                    value={r.value}
                    onChange={(e) => patchRow(i, { value: e.target.value })}
                    className="h-6 font-mono"
                  />
                </td>
                <td className="w-8 py-1">
                  <Button size="iconSm" variant="ghost" title="Remove" onClick={() => removeRow(i)}>
                    ×
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <Button size="sm" variant="ghost" onClick={addRow}>
          + Add header
        </Button>

        <div className="mb-1 mt-3 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
          Body
        </div>
        <textarea
          value={body}
          onChange={(e) => setBody(e.target.value)}
          spellCheck={false}
          className="h-64 w-full resize-y rounded border border-border bg-card p-2 font-mono text-xs"
        />
      </div>
    </div>
  );
}
